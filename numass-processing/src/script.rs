//! Скриптовый шаг предобработки на [ulua](https://github.com/webc-site/ulua) (Luau).
//!
//! Скрипт получает кадр таблицей Luau и возвращает обработанные отсчёты.
//! Канал представлен парой: таблица-снимок `channel.samples`, с которой
//! скрипт работает обычным синтаксисом (`samples[i]`, `#samples`), и скрытый
//! Luau `buffer` того же канала (f32 LE). Рантайм синхронизирует их JIT-циклами
//! прелюдии ([`PRELUDE`]): `snap` перед вызовом скрипта, `commit` после.
//! Тяжёлая математика хоста ([`ScriptHost`]) работает с буфером напрямую,
//! без обхода таблиц, и сама обновляет снимок.
//!
//! [`StaticPointParams`] передаётся глобалом `static_params` (зеркало
//! имени типа): таблица собирается один раз на прогон точки.
//!
//! VM создаётся для каждого вызова шага в потоке, выполнившем этот вызов.

use std::cell::RefCell;

#[cfg(any(feature = "egui", test))]
use ulua::TypeDiagnostic;
use ulua::rt::LuaSerdeExt;
use ulua::{Buffer, Compiler, Function, IntoLua, Lua, Table};

use crate::preprocess::{ProcessedFrame, ProcessedPoint, StaticPointParams, emulate_fir};

pub type ScriptError = ulua::Error;

/// Ключ, под которым канал-таблица хранит скрытый буфер.
///
/// Итерация `for _, channel in frame` проходит по кадру, а не по телу канала,
/// поэтому ключ не виден обычным скриптам.
const BUFFER_KEY: &str = "__buffer";

/// Копирует скрытый буфер канала в `Vec<f32>`.
///
/// Для host-функций, которым нужны отсчёты целиком: чтение через `to_vec`
/// стоит ~0.4 нс/отсчёт, в отличие от обхода таблицы (~23 нс/отсчёт).
pub(crate) fn channel_samples(body: &Table) -> Result<Vec<f32>, ScriptError> {
    let buffer: Buffer = body.raw_get(BUFFER_KEY)?;
    Ok(f32_from_bytes(&buffer.to_vec()))
}

/// Записывает `samples` в скрытый буфер канала и обновляет таблицу-снимок.
///
/// Буфер пересоздаётся, если длина изменилась. Снимок обновляется всегда,
/// так что скрипт сразу читает новые значения обычным синтаксисом.
pub(crate) fn set_channel_samples(
    lua: &Lua,
    body: &Table,
    samples: &[f32],
) -> Result<(), ScriptError> {
    let bytes = f32_to_bytes(samples);
    let mut buffer: Buffer = body.raw_get(BUFFER_KEY)?;
    if buffer.len() != bytes.len() {
        buffer = lua.create_buffer_with_capacity(bytes.len())?;
        body.raw_set(BUFFER_KEY, buffer.clone())?;
    }
    buffer.write_bytes(0, &bytes);

    let snap: Function = lua.globals().raw_get("__snap_channel")?;
    snap.call::<()>(body.clone())
}

/// Распаковывает f32 LE из байтов буфера.
fn f32_from_bytes(bytes: &[u8]) -> Vec<f32> {
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|chunk| f32::from_le_bytes(*chunk))
        .collect()
}

/// Упаковывает отсчёты в f32 LE для буфера.
fn f32_to_bytes(samples: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(samples.len() * 4);
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}

/// Прелюдия: синхронизация таблицы-снимка и скрытого буфера канала.
///
/// Обоснование Luau-стороны (JIT циклов `buffer.*`) — в шапке файла.
const PRELUDE: &str = include_str!("scripts/prelude.luau");

/// Definitions API скриптов preprocess-шага: типы `static_params`, `host`,
/// кадра и канала. Единственный источник истины для тайпчека ([`check_script`]) и
/// автодополнения ([`completion_words`]); подробности — в шапке
/// `scripts/defs.d.luau`.
#[cfg(any(feature = "egui", test))]
pub(crate) const SCRIPT_DEFS: &str = include_str!("scripts/defs.d.luau");

/// Разобранный [`SCRIPT_DEFS`]: глобалы с членами и объявленные типы.
///
/// Формат контролируется нами (`export type NAME = { ... }`,
/// `declare NAME: { ... }` без вложенных блоков), поэтому хватает
/// построчного сканера; корректность синтаксиса дополнительно проверяет
/// тайпчекер в `defs_typecheck_diagnostics_shapes`.
#[cfg(any(feature = "egui", test))]
#[derive(Debug, Default)]
pub(crate) struct DefsApi {
    /// Глобалы `declare NAME: { ... }` и члены `export type NAME = {`.
    pub(crate) globals: Vec<(String, Vec<String>)>,
    /// Имена `export type NAME = {` (Channel, Frame).
    pub(crate) types: Vec<String>,
}

#[cfg(any(feature = "egui", test))]
impl DefsApi {
    /// Члены глобала по имени (`host`, `static_params`, `Channel`).
    #[cfg(test)]
    pub(crate) fn members_of(&self, global: &str) -> Vec<String> {
        self.globals
            .iter()
            .find(|(name, _)| name == global)
            .map(|(_, members)| members.clone())
            .unwrap_or_default()
    }
}

/// Построчный разбор [`SCRIPT_DEFS`] по контролируемому формату.
#[cfg(any(feature = "egui", test))]
fn parse_defs(defs: &str) -> DefsApi {
    let mut api = DefsApi::default();
    let mut block: Option<(String, Vec<String>)> = None;
    for line in defs.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("export type ") {
            let name = rest.split_once('=').map_or(rest, |(name, _)| name).trim();
            api.types.push(name.to_owned());
            block = Some((name.to_owned(), Vec::new()));
        } else if let Some(rest) = line.strip_prefix("declare ") {
            if let Some((name, _)) = rest.split_once(':') {
                block = Some((name.trim().to_owned(), Vec::new()));
            }
        } else if block.is_some() && line == "}" {
            api.globals.push(block.take().expect("checked is_some"));
        } else if let Some((_, members)) = block.as_mut() {
            // Индексаторы (`[number]: Channel`) и комментарии — не члены.
            if let Some((member, _)) = line.split_once(':') {
                let member = member.trim();
                if !member.is_empty() && !member.starts_with('[') && !member.starts_with("--") {
                    members.push(member.to_owned());
                }
            }
        }
    }
    api
}

/// Слова автодополнения редактора скриптов.
///
/// Механически извлекаются из [`SCRIPT_DEFS`]: члены `host`/`static_params`/
/// extern-типов и сами имена. Bare-имена, а не
/// `host.fir`: Completer egui_code_editor достраивает слово по префиксу от
/// последнего не-словесного символа, так что после `host.` вводится `f`.
#[cfg(feature = "egui")]
pub(crate) fn completion_words() -> &'static [String] {
    static WORDS: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    WORDS.get_or_init(|| {
        let api = parse_defs(SCRIPT_DEFS);
        let mut words = api.types.clone();
        for (name, members) in &api.globals {
            words.push(name.clone());
            words.extend(members.iter().cloned());
        }
        words.sort_unstable();
        words.dedup();
        words
    })
}

/// Тайпчек исходника против [`SCRIPT_DEFS`].
///
/// Пустой результат — ошибок нет. Сообщения несут подсказки по сигнатурам
/// host-функций (рендер всей таблицы при опечатке, arity при незавершённом
/// вызове) — см. тест `defs_typecheck_diagnostics_shapes`.
#[cfg(any(feature = "egui", test))]
pub(crate) fn check_script(source: &str) -> Vec<TypeDiagnostic> {
    ulua::check_with_definitions(source, SCRIPT_DEFS)
        .err()
        .unwrap_or_default()
}

/// Набор функций, доступных скрипту через глобал `host`.
///
/// [`install`](ScriptHost::install) вызывается один раз при сборке VM.
/// Реализация регистрирует тяжёлую математику, свёртки, отрисовку графиков
/// и прочие операции, которые не выражаются циклом Luau. Сам шаг про имена
/// этих функций ничего не знает.
pub trait ScriptHost {
    fn install(&self, lua: &Lua, host: &Table) -> Result<(), ScriptError>;
}

/// Хост шага предобработки.
///
/// Не хранит состояния: таблица `host` принадлежит одной VM.
#[derive(Clone, Copy, Debug, Default)]
pub struct PreprocessHost;

impl ScriptHost for PreprocessHost {
    fn install(&self, lua: &Lua, host: &Table) -> Result<(), ScriptError> {
        // FIR над скрытым буфером канала: Table -> to_vec -> emulate_fir ->
        // новый буфер -> обновление снимка. Скрипт передаёт тело канала
        // (`frame[ch]`), окно задаётся как в `PreprocessStep::Fir`.
        host.set(
            "fir",
            lua.create_function(|lua, (body, left, center, right): (Table, u16, u16, u16)| {
                let samples = channel_samples(&body)?;
                let filtered =
                    emulate_fir(&samples, right as usize, center as usize, left as usize);
                set_channel_samples(lua, &body, &filtered)?;

                // `emulate_fir` укорачивает сигнал на `width - 1` отсчётов.
                let width = usize::from(left + center + right);
                let delay: i64 = body.raw_get("delay")?;
                body.raw_set("delay", delay + width as i64)?;
                Ok(())
            })?,
        )?;
        Ok(())
    }
}

/// Канал одного кадра: таблицы и скрытый буфер, переиспользуемые между кадрами.
///
/// Длина кадра в точке постоянна, поэтому буфер выделяется один раз, а на
/// каждом кадре перезаписывается. `snap`/`commit` переносят значения между
/// буфером и таблицей-снимком.
struct ChannelScratch {
    body: Table,
    samples: Table,
}

/// Скомпилированный скрипт и VM, в которой он выполняется.
///
/// `Lua` не реализует `Send`, поэтому рантайм используется только в потоке,
/// создавшем шаг.
pub struct ScriptRuntime {
    lua: Lua,
    /// `function(frame)`, возвращённая скриптом.
    process: Function,
    /// `snap_channel(body)` из прелюдии: буфер -> таблица.
    snap: Function,
    /// `commit_channel(body)` из прелюдии: таблица -> буфер.
    commit: Function,
    /// Таблицы первого кадра. Между кадрами не пересоздаются.
    scratch: RefCell<Option<(Table, Vec<ChannelScratch>)>>,
}

impl ScriptRuntime {
    /// Компилирует `source` на максимальном уровне оптимизации Luau.
    ///
    /// Скрипт обязан вернуть `function(frame)`. До вызова хост выставляет
    /// глобалы:
    /// - `static_params` — таблица из [`StaticPointParams`] или `nil`;
    /// - `host` — таблица функций [`ScriptHost`]; прелюдия добавляет в неё
    ///   `refresh(ch)` (буфер -> снимок), если хост не определил свою.
    pub fn compile(source: &str, host: &dyn ScriptHost) -> Result<Self, ScriptError> {
        let lua = Lua::new();
        // JIT доступен только на native A64/x64 при включённой фиче.
        // Если он недоступен, используется интерпретатор.
        #[cfg(all(
            not(target_arch = "wasm32"),
            any(target_arch = "x86_64", target_arch = "aarch64")
        ))]
        {
            let _ = lua.enable_jit(true);
        }

        // Уровень 2 — максимум оптимизации компилятора Luau.
        // Отладочная информация и покрытие увеличивают байткод.
        let compiler = Compiler::new()
            .set_optimization_level(2)
            .set_debug_level(0)
            .set_coverage_level(0)
            .set_type_info_level(0);
        lua.set_compiler(compiler.clone());

        let host_table = lua.create_table();
        host.install(&lua, &host_table)?;

        let prelude: Table = lua
            .load(PRELUDE)
            .set_name("prelude")
            .set_compiler(compiler.clone())
            .eval()?;
        let snap: Function = prelude.get("snap_channel")?;
        let commit: Function = prelude.get("commit_channel")?;
        if host_table.get::<Option<Function>>("refresh")?.is_none() {
            host_table.set("refresh", prelude.get::<Function>("refresh")?)?;
        }
        // Хост-функции вызывают snap после записи в буфер.
        lua.globals().raw_set("__snap_channel", snap.clone())?;

        lua.globals().set("host", host_table)?;

        let process: Function = lua
            .load(source)
            .set_name("preprocess")
            .set_compiler(compiler)
            .eval()?;

        Ok(Self {
            lua,
            process,
            snap,
            commit,
            scratch: RefCell::new(None),
        })
    }

    /// Выполняет скрипт для каждого кадра точки.
    ///
    /// Таблица `static_params` собирается один раз на вызов. Отсчёты кадра
    /// записываются в буферы рантайма, копируются в таблицы-снимки, а после
    /// вызова `commit` переносит правки скрипта обратно в буферы.
    pub fn apply(
        &self,
        frames: &mut ProcessedPoint,
        point: Option<&StaticPointParams>,
    ) -> Result<(), ScriptError> {
        let point: ulua::Value = match point {
            Some(point) => self.lua.to_value(point)?,
            None => ulua::Nil.into_lua(&self.lua)?,
        };
        self.lua.globals().set("static_params", point)?;

        for frame in frames.values_mut() {
            let table = self.fill(frame)?;
            self.process.call::<()>(table)?;
            self.read_back(frame)?;
        }
        Ok(())
    }

    fn fill(&self, frame: &ProcessedFrame) -> Result<Table, ScriptError> {
        let mut scratch = self.scratch.borrow_mut();
        let (table, channels) = scratch.get_or_insert_with(|| {
            (
                self.lua.create_table_with_capacity(frame.len() + 1, 1),
                Vec::with_capacity(frame.len()),
            )
        });

        if channels.len() != frame.len() {
            channels.clear();
            table.clear()?;
            for (id, channel) in frame {
                let buffer = self
                    .lua
                    .create_buffer_with_capacity(channel.samples.len() * 4)?;
                let samples = self
                    .lua
                    .create_table_with_capacity(channel.samples.len(), 0);
                let body = self.lua.create_table_with_capacity(0, 3);
                body.raw_set("samples", samples.clone())?;
                body.raw_set(BUFFER_KEY, buffer.clone())?;
                table.raw_set(i64::from(*id), body.clone())?;
                channels.push(ChannelScratch { body, samples });
            }
        }

        for (slot, (_, channel)) in channels.iter_mut().zip(frame) {
            slot.body.raw_set("delay", channel.delay)?;
            // Буфер живёт в теле канала: host-функции могут заменить его
            // (например, `host.fir` укорачивает сигнал).
            let mut buffer: Buffer = slot.body.raw_get(BUFFER_KEY)?;
            // Длина кадра изменилась между точками: буфер выделяется заново.
            if buffer.len() != channel.samples.len() * 4 {
                let fresh = self
                    .lua
                    .create_buffer_with_capacity(channel.samples.len() * 4)?;
                slot.body.raw_set(BUFFER_KEY, fresh.clone())?;
                buffer = fresh;
            }
            buffer.write_bytes(0, &f32_to_bytes(&channel.samples));

            // Буфер -> таблица-снимок: скрипт видит обычную таблицу.
            self.snap.call::<()>((slot.body.clone(),))?;
        }
        Ok(table.clone())
    }

    fn read_back(&self, frame: &mut ProcessedFrame) -> Result<(), ScriptError> {
        let scratch = self.scratch.borrow();
        let Some((_, channels)) = scratch.as_ref() else {
            return Ok(());
        };
        for (slot, channel) in channels.iter().zip(frame.values_mut()) {
            channel.delay = slot.body.raw_get::<i64>("delay")?.max(0) as usize;
            // Таблица -> буфер: правки скрипта и host-функций попадают
            // в отсчёты кадра.
            self.commit.call::<()>((slot.body.clone(),))?;
            let len = slot.samples.raw_len();
            let buffer: Buffer = slot.body.raw_get(BUFFER_KEY)?;
            channel.samples = f32_from_bytes(&buffer.to_vec())
                .into_iter()
                .take(len)
                .collect();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{
        cell::Cell,
        collections::{BTreeMap, BTreeSet},
        rc::Rc,
    };

    use super::*;
    use crate::preprocess::{AVERAGE_SCRIPT, ChannelSamples};

    /// Пользовательский хост регистрируется при сборке VM и доступен
    /// во всех кадрах одного рантайма.
    #[test]
    fn host_is_installed_once_per_vm() {
        struct MarkHost {
            installs: Rc<Cell<u32>>,
            calls: Rc<Cell<u32>>,
        }

        impl ScriptHost for MarkHost {
            fn install(&self, lua: &Lua, host: &Table) -> Result<(), ScriptError> {
                self.installs.set(self.installs.get() + 1);
                let calls = self.calls.clone();
                host.set(
                    "mark",
                    lua.create_function(move |_, ()| {
                        calls.set(calls.get() + 1);
                        Ok(7.0)
                    })?,
                )?;
                Ok(())
            }
        }

        let host = MarkHost {
            installs: Rc::new(Cell::new(0)),
            calls: Rc::new(Cell::new(0)),
        };
        let runtime = ScriptRuntime::compile(
            include_str!("scripts/tests/host_is_installed_once_per_vm.luau"),
            &host,
        )
        .unwrap();

        let channel = || ChannelSamples {
            samples: vec![1.0, 2.0],
            delay: 0,
        };
        let mut frames = BTreeMap::from([
            (0u64, BTreeMap::from([(0u8, channel())])),
            (1, BTreeMap::from([(0u8, channel())])),
        ]);
        runtime.apply(&mut frames, None).unwrap();

        assert_eq!(host.installs.get(), 1);
        assert_eq!(host.calls.get(), 2);
        assert_eq!(frames[&0][&0].samples[0], 7.0);
        assert_eq!(frames[&1][&0].samples[0], 7.0);
    }

    /// Скрипт продолжает работать с таблицей после host-операции: `host.fir`
    /// обновляет снимок сам, без ручного `refresh`.
    #[test]
    fn host_fir_refreshes_snapshot() {
        let runtime = ScriptRuntime::compile(
            include_str!("scripts/tests/host_fir_refreshes_snapshot.luau"),
            &PreprocessHost,
        )
        .unwrap();

        let samples: Vec<f32> = (0..16).map(|i| i as f32).collect();
        let mut frames = BTreeMap::from([(
            0u64,
            BTreeMap::from([(
                0u8,
                ChannelSamples {
                    samples: samples.clone(),
                    delay: 0,
                },
            )]),
        )]);
        runtime.apply(&mut frames, None).unwrap();

        let expected: Vec<f32> = emulate_fir(&samples, 1, 0, 1)
            .iter()
            .map(|v| v * 2.0)
            .collect();
        let channel = &frames[&0][&0];
        assert_eq!(channel.samples, expected);
        // `host.fir` увеличивает delay на ширину окна, как `PreprocessStep::Fir`.
        assert_eq!(channel.delay, 2);
    }

    /// `host.fir` без последующих правок даёт те же числа, что нативный FIR.
    #[test]
    fn host_fir_matches_native_fir() {
        let runtime = ScriptRuntime::compile(
            include_str!("scripts/tests/host_fir_matches_native_fir.luau"),
            &PreprocessHost,
        )
        .unwrap();

        let samples: Vec<f32> = (0..64).map(|i| (i * 7 % 23) as f32 - 11.0).collect();
        let mut frames = BTreeMap::from([(
            0u64,
            BTreeMap::from([(
                0u8,
                ChannelSamples {
                    samples: samples.clone(),
                    delay: 3,
                },
            )]),
        )]);
        runtime.apply(&mut frames, None).unwrap();

        let expected = emulate_fir(&samples, 6, 15, 6);
        let channel = &frames[&0][&0];
        assert_eq!(channel.samples, expected);
        assert_eq!(channel.delay, 3 + 27);
    }

    /// Длина кадра меняется между точками: буферы и снимки перестраиваются,
    /// значения не смешиваются с прошлым кадром.
    #[test]
    fn channel_length_change_between_points() {
        let runtime = ScriptRuntime::compile(
            include_str!("scripts/tests/channel_length_change_between_points.luau"),
            &PreprocessHost,
        )
        .unwrap();

        let mut frames = BTreeMap::from([
            (
                0u64,
                BTreeMap::from([(
                    0u8,
                    ChannelSamples {
                        samples: vec![1.0, 2.0, 3.0],
                        delay: 0,
                    },
                )]),
            ),
            (
                1,
                BTreeMap::from([(
                    0u8,
                    ChannelSamples {
                        samples: vec![4.0, 5.0],
                        delay: 0,
                    },
                )]),
            ),
            (
                2,
                BTreeMap::from([(
                    0u8,
                    ChannelSamples {
                        samples: vec![6.0, 7.0, 8.0, 9.0],
                        delay: 0,
                    },
                )]),
            ),
        ]);
        runtime.apply(&mut frames, None).unwrap();

        assert_eq!(frames[&0][&0].samples, vec![-1.0, 2.0, 3.0]);
        assert_eq!(frames[&1][&0].samples, vec![-1.0, 5.0]);
        assert_eq!(frames[&2][&0].samples, vec![-1.0, 7.0, 8.0, 9.0]);
    }

    /// Скрипт может обрезать канал: `commit` пишет ровно `#samples` значений,
    /// `read_back` возвращает укороченный кадр.
    #[test]
    fn script_can_trim_channel() {
        let runtime = ScriptRuntime::compile(
            include_str!("scripts/tests/script_can_trim_channel.luau"),
            &PreprocessHost,
        )
        .unwrap();

        let mut frames = BTreeMap::from([(
            0u64,
            BTreeMap::from([(
                0u8,
                ChannelSamples {
                    samples: vec![1.0, 2.0, 3.0],
                    delay: 0,
                },
            )]),
        )]);
        runtime.apply(&mut frames, None).unwrap();

        assert_eq!(frames[&0][&0].samples, vec![1.0, 2.0]);
    }

    /// `host.refresh` вручную подтягивает буфер в снимок — для host-функций,
    /// которые не обновляют снимок сами.
    #[test]
    fn host_refresh_updates_snapshot() {
        struct RawHost;

        impl ScriptHost for RawHost {
            fn install(&self, lua: &Lua, host: &Table) -> Result<(), ScriptError> {
                host.set(
                    "double",
                    lua.create_function(|_, body: Table| {
                        let mut buffer: Buffer = body.raw_get(BUFFER_KEY)?;
                        let mut bytes = buffer.to_vec();
                        for chunk in bytes.as_chunks_mut::<4>().0 {
                            let v = f32::from_le_bytes(*chunk);
                            chunk.copy_from_slice(&(v * 2.0).to_le_bytes());
                        }
                        buffer.write_bytes(0, &bytes);
                        Ok(())
                    })?,
                )?;
                Ok(())
            }
        }

        let runtime = ScriptRuntime::compile(
            include_str!("scripts/tests/host_refresh_updates_snapshot.luau"),
            &RawHost,
        )
        .unwrap();

        let mut frames = BTreeMap::from([(
            0u64,
            BTreeMap::from([(
                0u8,
                ChannelSamples {
                    samples: vec![1.0, 2.0],
                    delay: 0,
                },
            )]),
        )]);
        runtime.apply(&mut frames, None).unwrap();

        assert_eq!(frames[&0][&0].samples, vec![102.0, 4.0]);
    }

    /// Пустой кадр: скрипт получает пустую таблицу, шаг не падает.
    #[test]
    fn empty_frame_is_passed_through() {
        let runtime = ScriptRuntime::compile(
            include_str!("scripts/tests/empty_frame_is_passed_through.luau"),
            &PreprocessHost,
        )
        .unwrap();

        let mut frames = BTreeMap::from([(0u64, BTreeMap::<u8, ChannelSamples>::new())]);
        runtime.apply(&mut frames, None).unwrap();

        assert!(frames[&0].is_empty());
    }

    /// Формы сообщений тайпчека против [`SCRIPT_DEFS`]: на них держится дизайн
    /// редактора (Completer из definitions + панель ошибок как signature help).
    /// Позитивный кейс заодно валидирует сам definitions-синтаксис.
    #[test]
    fn defs_typecheck_diagnostics_shapes() {
        let clean = "return function(frame: Frame) host.fir(frame[1], 1, 1, 1) end";
        assert!(check_script(clean).is_empty());

        let cases = [
            (
                "unknown member",
                "return function(frame: Frame) host.firr(frame[1]) end",
            ),
            (
                "zero-arg probe",
                "return function(frame: Frame) host.fir() end",
            ),
            (
                "wrong types",
                "return function(frame: Frame) host.fir(frame[1], \"x\", 1, 2) end",
            ),
            (
                "unknown static_params field",
                "return function(frame: Frame) return static_params.hvv end",
            ),
            (
                "incomplete call (while typing)",
                "return function(frame: Frame) host.fir(",
            ),
        ];
        for (name, source) in cases {
            let diags = check_script(source);
            if diags.is_empty() {
                println!("{name}: unexpectedly OK");
            }
            for d in &diags {
                println!("{name} @ {}:{}: {}", d.line, d.column, d.message);
            }
        }

        // Пустой исходник валиден: редактор стартует без диагностики.
        assert!(check_script("").is_empty());

        // Инвариант 1: опечатка в члене таблицы рендерит в сообщении
        // сигнатуры всех её функций — панель ошибок удваивается как подсказка
        // по API. Имена параметров рендерер теряет (типы остаются).
        let diags = check_script("return function(frame: Frame) host.firr(frame[1]) end");
        assert!(diags.iter().any(|d| {
            d.message
                .contains("fir: (Channel, number, number, number) -> ()")
        }));

        // Инвариант 2: незавершённый вызов во время набора уже даёт
        // arity-подсказку — параллельно с ошибкой парсинга, без паники.
        let diags = check_script("return function(frame: Frame) host.fir(");
        assert!(
            diags
                .iter()
                .any(|d| d.message.contains("expects 4 arguments"))
        );

        // Стоимость одного вызова (front-end + builtins + defs парсятся каждый
        // раз — переиспользуемый `Checker` definitions не поддерживает):
        // ориентир для частоты проверки в виджете, включая wasm.
        let started = std::time::Instant::now();
        for _ in 0..10 {
            let _ = check_script(clean);
        }
        println!(
            "check_with_definitions: {:.1} ms/call",
            started.elapsed().as_secs_f64() * 100.0 / 10.0
        );
    }

    /// [`SCRIPT_DEFS`] покрывает host-функции рантайма без пропусков и
    /// фантомов: имена собираются интроспекцией VM после `ScriptHost::install`
    /// (включая `refresh` прелюдии) и сравниваются с `declare host`.
    /// Новая host-функция без строки в definitions ломает этот тест.
    #[test]
    fn defs_cover_host_functions() {
        let lua = Lua::new();
        let host = lua.create_table();
        PreprocessHost.install(&lua, &host).unwrap();
        // Прелюдия добавляет refresh при отсутствии своей (ScriptRuntime::compile).
        if host.get::<Option<Function>>("refresh").unwrap().is_none() {
            host.set("refresh", lua.create_function(|_, ()| Ok(())).unwrap())
                .unwrap();
        }

        let mut registered = BTreeSet::new();
        host.for_each::<String, ulua::Value>(|key, _| {
            registered.insert(key);
            Ok(())
        })
        .unwrap();

        let declared = parse_defs(SCRIPT_DEFS).members_of("host");
        assert_eq!(
            registered,
            declared.into_iter().collect::<BTreeSet<String>>()
        );
    }

    /// [`SCRIPT_DEFS`] покрывает поля `static_params`: ключи таблицы собираются той же
    /// сериализацией, которой рантайм выставляет глобал (ScriptRuntime::apply).
    /// Новое поле `StaticPointParams` без строки в definitions ломает этот тест.
    #[test]
    fn defs_cover_point_fields() {
        use crate::types::{Time, second};

        let sample = StaticPointParams {
            hv: 14_000.0,
            start_time: chrono::DateTime::from_timestamp(0, 0).unwrap().naive_utc(),
            acquisition_time: Time::new::<second>(30.0),
            frame_len: Time::new::<second>(40.0),
            bad_blocks: BTreeSet::from([3]),
        };
        let lua = Lua::new();
        let point = lua.to_value(&sample).unwrap();
        let table = point.as_table().unwrap();

        let mut serialized = BTreeSet::new();
        table
            .for_each::<String, ulua::Value>(|key, _| {
                serialized.insert(key);
                Ok(())
            })
            .unwrap();

        let declared = parse_defs(SCRIPT_DEFS).members_of("static_params");
        assert_eq!(
            serialized,
            declared.into_iter().collect::<BTreeSet<String>>()
        );
    }

    /// Прелюдия и тестовые скрипты, использующие только объявленный в
    /// SCRIPT_DEFS API, проходят тайпчек — аннотации в .luau-файлах валидны.
    /// Скрипты с незадекларированными host-функциями (`host.mark`,
    /// `host.double`) не проверяются: они вне definitions по построению.
    #[test]
    fn embedded_scripts_typecheck() {
        for (name, source) in [
            ("prelude", PRELUDE),
            (
                "host_fir_refreshes_snapshot",
                include_str!("scripts/tests/host_fir_refreshes_snapshot.luau"),
            ),
            (
                "host_fir_matches_native_fir",
                include_str!("scripts/tests/host_fir_matches_native_fir.luau"),
            ),
            (
                "channel_length_change_between_points",
                include_str!("scripts/tests/channel_length_change_between_points.luau"),
            ),
            (
                "script_can_trim_channel",
                include_str!("scripts/tests/script_can_trim_channel.luau"),
            ),
            (
                "empty_frame_is_passed_through",
                include_str!("scripts/tests/empty_frame_is_passed_through.luau"),
            ),
        ] {
            let diags = check_script(source);
            assert!(
                diags.is_empty(),
                "{name}: unexpected diagnostics: {diags:?}"
            );
        }
    }

    /// Дефолтный скрипт (с аннотацией `frame: Frame`) проходит тайпчек:
    /// редактор стартует без диагностики на свежем шаге `+ script`.
    #[test]
    fn average_script_typechecks() {
        let diags = check_script(AVERAGE_SCRIPT);
        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
    }
}
