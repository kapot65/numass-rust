//! Исполнение preprocess-скриптов вне главного wasm-модуля.
//!
//! Lua-ошибки в ulua — это паники (`lua_d_throw` → `panic_any(lua_exception)`),
//! ловимые `catch_unwind` на границах protected call. На wasm32-unknown-unknown
//! дефолтная стратегия — `panic = abort`: перехвата нет, и любой `error()` в
//! пользовательском скрипте уронил бы весь wasm-модуль (trap) и заморозил
//! интерфейс. Поэтому на web цепочка шагов предобработки исполняется в
//! отдельном worker-модуле (bin `script-worker` → ассеты `script-worker.js` +
//! `script-worker_bg.wasm`): trap убивает только воркер, главный модуль
//! остаётся жив и показывает ошибку.
//!
//! Протокол — MessagePack поверх postMessage:
//! - запрос [`ScriptTask`] (кадры i16-кодов, шаги, параметры точки);
//! - ответ [`ScriptReply`], `Err` — полнотекстовая ошибка с префиксом
//!   `preprocess script: ...`;
//! - синтаксис/компиляция: typecheck-гейт отвечает текстом, не исполняя скрипт;
//! - READY-рукопожатие: задачи копятся в очереди, пока воркер не подтвердит,
//!   что установил `onmessage`, — иначе первые задачи теряются молча;
//! - runtime-throw добивает воркер; главный поток узнает через `onerror` и
//!   получает `Err` для всех активных задач, воркер пересоздается лениво;
//!   так же трактуется и неуспешная wasm-инициализация воркера (`INIT_ERROR`).
//!
//! На нативе воркер не входит в исполнение: [`apply_waveforms_with`] здесь —
//! прямой вызов `numass_processing::preprocess::apply_waveforms_with`. Одна
//! сигнатура у обеих платформ, вся разница — где физически живут скрипты.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use numass_processing::{
    error::PipelineError,
    preprocess::{PreprocessStep, ProcessedPoint, StaticPointParams},
    types::NumassWaveformsFast,
};

#[cfg(target_arch = "wasm32")]
use {
    std::cell::RefCell,
    wasm_bindgen::{JsCast, JsValue, closure::Closure},
    wasm_bindgen_futures::JsFuture,
    web_sys::{Blob, BlobPropertyBag, DedicatedWorkerGlobalScope, MessageEvent, Url, Worker},
};

#[cfg(target_arch = "wasm32")]
const DEATH_TEXT: &str =
    "preprocess script: скрипт уронил worker (runtime fault); точная причина — в консоли браузера";

/// Задача worker'а: исполнить цепочку шагов предобработки (включая Script).
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct ScriptTask {
    pub(crate) id: u64,
    pub(crate) steps: Vec<PreprocessStep>,
    /// Кадры точки в i16-кодах.
    pub(crate) frames: BTreeMap<u64, BTreeMap<u8, Vec<i16>>>,
    /// Параметры точки для глобала `static_params`.
    pub(crate) point: Option<StaticPointParams>,
}

/// Ответ worker'а. `Err` — полнотекстовая ошибка шага.
#[derive(Serialize, Deserialize)]
pub(crate) struct ScriptReply {
    pub(crate) id: u64,
    pub(crate) result: Result<ProcessedPoint, String>,
}

/// Исполнить цепочку шагов, включая скриптовый шаг.
///
/// wasm: цепочка с Script-шагом целиком уезжает в worker-модуль (копия
/// i16-кадров); без Script-шага — локальный проход без копий, как native.
///
/// Одинаковая сигнатура у обеих платформ: вьюверы зовут этот вызов изоморфно,
/// и `point` в скрипте — тот же глобал, что и у прямого прогона.
pub(crate) async fn apply_waveforms_with(
    waveforms: &NumassWaveformsFast<'_>,
    steps: &[PreprocessStep],
    point: Option<&StaticPointParams>,
) -> Result<ProcessedPoint, PipelineError> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        return numass_processing::preprocess::apply_waveforms_with(waveforms, steps, point);
    }

    #[cfg(target_arch = "wasm32")]
    {
        // Без Script-шага паники быть нечему: гоняем локально, без копий кадров
        // и без worker'а — иначе каждая точка платила бы копией на десятки МБ.
        if !steps
            .iter()
            .any(|s| matches!(s, PreprocessStep::Script { .. }))
        {
            return numass_processing::preprocess::apply_waveforms_with(waveforms, steps, point);
        }
        let task = ScriptTask {
            id: reserve_id(),
            steps: steps.to_vec(),
            // Копия кадров (в FAST-виде отсчеты заимствованы из точки).
            frames: waveforms.iter().map(copy_frame).collect(),
            point: point.cloned(),
        };
        let request = rmp_serde::to_vec(&task).expect("ScriptTask serialization");
        let reply_bytes = await_reply(task.id, &request).await;
        let reply: ScriptReply = rmp_serde::from_slice(&reply_bytes).expect("ScriptReply decoding");
        reply.result.map_err(PipelineError::Data)
    }
}

#[cfg(target_arch = "wasm32")]
fn copy_frame((time, frame): (&u64, &BTreeMap<u8, &[i16]>)) -> (u64, BTreeMap<u8, Vec<i16>>) {
    (
        *time,
        frame
            .iter()
            .map(|(channel, samples)| (*channel, samples.to_vec()))
            .collect(),
    )
}

// ---------------------------------------------------------------------------
// worker side (bin `script-worker`)
// ---------------------------------------------------------------------------

/// Точка входа bin `script-worker`: подписка на postMessage
/// (тело — wasm-only; нативная сборка bin'а отбивает паникой).
#[cfg(target_arch = "wasm32")]
pub fn run_script_worker() {
    let scope: DedicatedWorkerGlobalScope = js_sys::global().unchecked_into();
    let scope_for_post = scope.clone();
    let on_message = Closure::wrap(Box::new(move |event: MessageEvent| {
        let bytes = event.data().unchecked_into::<js_sys::Uint8Array>().to_vec();
        let Ok(task) = rmp_serde::from_slice::<ScriptTask>(&bytes) else {
            return; // битая задача: в протоколе это ошибка транспорта, не скрипта
        };
        let reply = dispatch(task);
        // Ok: ответ уходит; Err-serialization невозможен и не ожидается.
        if let Ok(payload) = rmp_serde::to_vec(&reply) {
            let _ = scope_for_post.post_message(&js_sys::Uint8Array::from(&payload[..]));
        }
    }) as Box<dyn FnMut(MessageEvent)>);
    scope.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
    on_message.forget();
    // Рукопожатие: главный модуль шлет задачи только после этого сообщения,
    // чтобы задачи, отправленные до установки onmessage, не терялись.
    let _ = scope.post_message(&js_sys::JsString::from("READY").into());
}

/// Исполнить задачу worker'а: typecheck-гейт, потом цепочка шагов
/// (включая скриптовый шаг). Одна реализация для wasm-worker'а и для
/// нативных тестов протокола.
///
/// Нативно вне теста протокола функция мертва (весь glue под cfg(wasm32)) —
/// отсюда `allow(dead_code)`.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub(crate) fn dispatch(task: ScriptTask) -> ScriptReply {
    // Гейт type-check: ошибки сигнатур/аргументов/nil-полей не доходят до
    // исполнения, а возвращаются текстом (на wasm иначе это был бы trap worker'а).
    for step in &task.steps {
        if let PreprocessStep::Script { source } = step
            && let Err(diagnostics) = numass_processing::script::typecheck_script(source)
        {
            return ScriptReply {
                id: task.id,
                result: Err(format!("preprocess script: {diagnostics}")),
            };
        }
    }

    // FAST-вид поверх собранных кадров — то же, что делает extract_waveforms.
    let waveforms: NumassWaveformsFast<'_> = task
        .frames
        .iter()
        .map(|(time, frame)| {
            (
                *time,
                frame
                    .iter()
                    .map(|(channel, samples)| (*channel, samples.as_slice()))
                    .collect(),
            )
        })
        .collect();

    match numass_processing::preprocess::apply_waveforms_with(
        &waveforms,
        &task.steps,
        task.point.as_ref(),
    ) {
        Ok(processed) => ScriptReply {
            id: task.id,
            result: Ok(processed),
        },
        Err(err) => ScriptReply {
            id: task.id,
            result: Err(err.to_string()),
        },
    }
}

// ---------------------------------------------------------------------------
// main side: состояние воркера, ленивый запуск, ответ-фьючерсы
// ---------------------------------------------------------------------------

#[cfg(target_arch = "wasm32")]
struct ScriptWorkerState {
    worker: Option<Worker>,
    /// id → resolve-колбек JS-промиса ожидания ответа.
    pending: BTreeMap<u64, js_sys::Function>,
    next_id: u64,
    /// Рукопожатие: true после `READY` от воркера; до этого задачи в очереди.
    ready: bool,
    /// Задачи, ждущие готовности воркера; их resolve-функции уже в pending.
    queued: Vec<js_sys::Uint8Array>,
}

#[cfg(target_arch = "wasm32")]
thread_local! {
    static SCRIPT_WORKER: RefCell<Option<ScriptWorkerState>> =
        const { RefCell::new(None) };
}

#[cfg(target_arch = "wasm32")]
fn with_state<T>(f: impl FnOnce(&mut ScriptWorkerState) -> T) -> T {
    SCRIPT_WORKER.with(|cell| {
        let mut state = cell.borrow_mut();
        f(state.get_or_insert_with(|| ScriptWorkerState {
            worker: None,
            pending: Default::default(),
            next_id: 0,
            ready: false,
            queued: Default::default(),
        }))
    })
}

#[cfg(target_arch = "wasm32")]
fn reserve_id() -> u64 {
    with_state(|state| {
        let id = state.next_id;
        state.next_id += 1;
        id
    })
}

#[cfg(target_arch = "wasm32")]
/// Смерть или неудачная инициализация воркера: все живые задачи (и посланные,
/// и ждущие READY — их id уже в pending) разрешаются текстом ошибки,
/// воркер будет пересоздан лениво при следующей задаче.
fn fail_worker(state: &mut ScriptWorkerState, message: &str) {
    state.worker = None;
    state.ready = false;
    state.queued.clear();
    let dying = state
        .pending
        .iter()
        .map(|(&id, resolve)| (id, resolve.clone()))
        .collect::<Vec<_>>();
    state.pending.clear();
    for (id, resolve) in dying {
        let payload = rmp_serde::to_vec(&ScriptReply {
            id,
            result: Err(message.to_owned()),
        })
        .expect("failure reply serialization");
        let payload = js_sys::Uint8Array::from(&payload[..]);
        let _ = resolve.call1(&JsValue::NULL, &payload);
    }
}

#[cfg(target_arch = "wasm32")]
fn spawn_worker() -> Worker {
    // Тот же способ запуска, что у gloo-worker (create_worker): web-glue
    // bindген'а не имеет автозапуска и не может найти wasm в worker-окружении
    // (document недоступен, import.meta в классике не валиден) — грузим js
    // классическим воркером из blob'а и явно звуем wasm_bindgen(wasm_url).
    // URL — как в storage::api_url: self.location.href через js_sys::eval.
    // NB: r#"..."# — raw-строка: кавычки в JS не экранируются, иначе получится
    // SyntaxError в blob'е и не загрузится ничего.
    let base = js_sys::eval("self.location.href")
        .expect("self.location.href")
        .as_string()
        .expect("location href is a string");
    let js_url = Url::new_with_base("./script-worker.js", &base)
        .expect("script-worker.js url")
        .to_string();
    let wasm_url = js_url.replace(".js", "_bg.wasm");

    // При неудачной инициализации wasm шлём INIT_ERROR: иначе READY не придёт
    // никогда и ожидающие задачи повиснут навечно (серый экран без ошибок).
    let bootstrap = format!(
        r#"importScripts("{js_url}"); wasm_bindgen("{wasm_url}").then(() => {{}}, e => {{ console.error('script-worker wasm init failed:', e); postMessage('INIT_ERROR ' + (e && e.message ? e.message : '' + e)); }});"#
    );
    let array = js_sys::Array::new();
    array.push(&bootstrap.into());
    let bag = BlobPropertyBag::new();
    bag.set_type("application/javascript");
    let blob = Blob::new_with_str_sequence_and_options(&array, &bag).expect("bootstrap blob");
    let url = Url::create_object_url_with_blob(&blob).expect("blob url");
    let worker = Worker::new(&url).expect("script-worker spawn");

    // Ответ / рукопожатие: READY переводит glue в готовность и выгружает очередь.
    let on_message = Closure::wrap(Box::new(move |event: MessageEvent| {
        let data = event.data();
        if let Some(text) = data.as_string() {
            if text == "READY" {
                with_state(|state| {
                    state.ready = true;
                    // Очередь выгружается только при живом воркере; смерть
                    // до READY провалит задачи через fail_worker (по onerror).
                    if let Some(worker) = state.worker.as_ref() {
                        for task in std::mem::take(&mut state.queued) {
                            let _ = worker.post_message(&task);
                        }
                    }
                });
            } else if let Some(err) = text.strip_prefix("INIT_ERROR ") {
                with_state(|state| fail_worker(state, err));
            }
            return; // ответ по протоколу всегда бинарный; строки — только рукопожатие
        }
        with_state(|state| {
            let bytes = data.unchecked_into::<js_sys::Uint8Array>().to_vec();
            let Ok(reply) = rmp_serde::from_slice::<ScriptReply>(&bytes) else {
                return;
            };
            if let Some(resolve) = state.pending.remove(&reply.id) {
                // Отдаём промису копию байт ответа (в исходном событии).
                let payload = js_sys::Uint8Array::from(&bytes[..]);
                let _ = resolve.call1(&JsValue::NULL, &payload);
            }
        });
    }) as Box<dyn FnMut(MessageEvent)>);
    worker.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
    on_message.forget();

    // Смерть воркера (wasm trap): все активные задачи = Err, воркер = пересоздадим.
    let on_error = Closure::wrap(Box::new(move |_error: JsValue| {
        with_state(|state| fail_worker(state, DEATH_TEXT));
    }) as Box<dyn FnMut(JsValue)>);
    worker.set_onerror(Some(on_error.as_ref().unchecked_ref()));
    on_error.forget();

    worker
}

#[cfg(target_arch = "wasm32")]
/// Отправить задачу и дождаться байт ответа.
async fn await_reply(id: u64, request: &[u8]) -> Vec<u8> {
    // resolve-колбек исполняется тут же, при Promise::new, поэтому borrow
    // конфликтов нет; слот нужен, чтобы достать его наружу из executor'а.
    let task_bytes = js_sys::Uint8Array::from(request);
    let resolve_slot: std::rc::Rc<RefCell<Option<js_sys::Function>>> = Default::default();
    let slot_for_closure = resolve_slot.clone();

    let promise = js_sys::Promise::new(&mut |resolve, _reject| {
        slot_for_closure.borrow_mut().replace(resolve);
    });
    let resolve = resolve_slot
        .borrow_mut()
        .take()
        .expect("promise executor runs synchronously");

    with_state(|state| {
        state.pending.insert(id, resolve);
        if state.worker.is_none() {
            state.worker = Some(spawn_worker());
        }
        if state.ready {
            let posted = state
                .worker
                .as_ref()
                .expect("worker set above")
                .post_message(&task_bytes)
                .is_ok();
            if !posted {
                // Постить некому: воркер мертв, а onerror еще не дошел до нас.
                fail_worker(state, DEATH_TEXT);
            }
        } else {
            // Воркер еще инициализируется: в очередь до READY-рукопожатия;
            // после READY очередь выгрузится туда как есть (без пересборки).
            state.queued.push(task_bytes);
        }
    });

    JsFuture::from(promise)
        .await
        .expect("reply promise resolves (on error path too)")
        .unchecked_into::<js_sys::Uint8Array>()
        .to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Полный пайплайн worker-задачи нативно: typecheck-гейт, Script-шаг,
    /// запись скрипта в кадр — тот же `dispatch`, что исполняет wasm-worker.
    #[test]
    fn dispatch_runs_script_writes_back_samples() {
        let task = ScriptTask {
            id: 7,
            steps: vec![PreprocessStep::Script {
                source: "return function(frame) frame[1].samples[1] = 42 end".into(),
            }],
            frames: BTreeMap::from([(0, BTreeMap::from([(1, vec![0i16, 1, 2, 3])]))]),
            point: None,
        };

        let reply = dispatch(task);
        let processed = reply.result.expect("dispatch must succeed");
        assert_eq!(processed[&0][&1].samples[0], 42.0);
    }

    /// Тайпчек-гейт раньше исполнения: ошибки сигнатур не приводят к прогону.
    #[test]
    fn dispatch_reports_typecheck_errors_as_text() {
        let task = ScriptTask {
            id: 9,
            steps: vec![PreprocessStep::Script {
                // host.lpf требует 1 аргумент в юанит: вызов без аргумента —
                // ошибка типов, воркер отвечает текстом и не исполняет скрипт.
                source: "return function(frame) host.lpf(frame[1]) end".into(),
            }],
            frames: BTreeMap::from([(0, BTreeMap::from([(0, vec![0i16, 1, 2, 3])]))]),
            point: None,
        };

        let reply = dispatch(task);
        let Err(text) = reply.result else {
            panic!("type error must produce Err reply");
        };
        assert!(text.contains("preprocess script:"), "{text}");
    }
}
