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
//! [`StaticPointParams`] передаётся глобалом `point`: таблица собирается
//! один раз на прогон точки.
//!
//! VM создаётся для каждого вызова шага в потоке, выполнившем этот вызов.

use std::cell::RefCell;

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
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes(chunk.try_into().unwrap()))
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
/// Компилируется тем же компилятором, что и пользовательский скрипт, поэтому
/// циклы `buffer.readf32`/`buffer.writef32` попадают под JIT. Rust-обход
/// таблицы через ulua-rt в ~13 раз медленнее такого цикла, так что
/// копирование выполняется на стороне Luau.
const PRELUDE: &str = r#"
local function snap_channel(body)
    local b = body.__buffer
    local s = body.samples
    local n = buffer.len(b) // 4
    -- Срезать хвост снимка, если кадр стал короче.
    for i = #s, n + 1, -1 do
        s[i] = nil
    end
    for i = 1, n do
        s[i] = buffer.readf32(b, (i - 1) * 4)
    end
end

local function commit_channel(body)
    local b = body.__buffer
    local s = body.samples
    for i = 1, #s do
        buffer.writef32(b, (i - 1) * 4, s[i])
    end
end

return {
    snap_channel = snap_channel,
    commit_channel = commit_channel,
    refresh = snap_channel,
}
"#;

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
    /// - `point` — таблица из [`StaticPointParams`] или `nil`;
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
    /// Таблица `point` собирается один раз на вызов. Отсчёты кадра
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
        self.lua.globals().set("point", point)?;

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
    use std::{cell::Cell, collections::BTreeMap, rc::Rc};

    use super::*;
    use crate::preprocess::ChannelSamples;

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
            "return function(frame) frame[0].samples[1] = host.mark() end",
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
            r#"
return function(frame)
    for _, channel in frame do
        host.fir(channel, 1, 0, 1)
        for i = 1, #channel.samples do
            channel.samples[i] = channel.samples[i] * 2
        end
    end
end
"#,
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
            "return function(frame) for _, channel in frame do host.fir(channel, 6, 15, 6) end end",
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
            "return function(frame) for _, channel in frame do channel.samples[1] = -1 end end",
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
            r#"
return function(frame)
    for _, channel in frame do
        channel.samples[#channel.samples] = nil
    end
end
"#,
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
                        for chunk in bytes.chunks_exact_mut(4) {
                            let v = f32::from_le_bytes(chunk.try_into().unwrap());
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
            r#"
return function(frame)
    for _, channel in frame do
        host.double(channel)
        host.refresh(channel)
        channel.samples[1] = channel.samples[1] + 100
    end
end
"#,
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
            "return function(frame) for _, channel in frame do channel.samples[1] = 0 end end",
            &PreprocessHost,
        )
        .unwrap();

        let mut frames = BTreeMap::from([(0u64, BTreeMap::<u8, ChannelSamples>::new())]);
        runtime.apply(&mut frames, None).unwrap();

        assert!(frames[&0].is_empty());
    }

    /// Черновик definitions-файла (`numass.d.luau` из docs/research/luau-lsp-editor.md)
    /// принимается тайпчекером ulua, а диагностики типовых ошибок несут
    /// подсказки по сигнатурам — на этих инвариантах держится дизайн редактора
    /// скриптов (заполнение Completer из definitions + панель ошибок).
    #[test]
    fn defs_typecheck_diagnostics_shapes() {
        let defs = r#"
            declare extern type Channel with
                samples: {number}
                delay: number
            end

            declare extern type Frame with
                [number]: Channel
            end

            declare point: {
                hv: number,
                start_time: string,
                acquisition_time: number,
                frame_len: number,
                bad_blocks: {number},
            }

            declare host: {
                fir: (channel: Channel, left: number, center: number, right: number) -> (),
                refresh: (channel: Channel) -> (),
            }
        "#;

        let clean = "return function(frame: Frame) host.fir(frame[1], 1, 1, 1) end";
        if let Err(diags) = ulua::check_with_definitions(clean, defs) {
            panic!("clean script must typecheck, got: {diags:?}");
        }

        let cases = [
            (
                "unknown member",
                "return function(frame: Frame) host.firr(frame[1]) end",
            ),
            (
                "zero-arg probe (signature?)",
                "return function(frame: Frame) host.fir() end",
            ),
            (
                "wrong types",
                "return function(frame: Frame) host.fir(frame[1], \"x\", 1, 2) end",
            ),
            (
                "unknown point field",
                "return function(frame: Frame) return point.hvv end",
            ),
            (
                "incomplete call (while typing)",
                "return function(frame: Frame) host.fir(",
            ),
        ];
        for (name, source) in cases {
            match ulua::check_with_definitions(source, defs) {
                Ok(()) => println!("{name}: unexpectedly OK"),
                Err(diags) => {
                    for d in &diags {
                        println!("{name} @ {}:{}: {}", d.line, d.column, d.message);
                    }
                }
            }
        }

        // Пустой исходник валиден: редактор стартует без диагностики.
        assert!(ulua::check_with_definitions("", defs).is_ok());

        // Инвариант 1: опечатка в члене таблицы рендерит в сообщении
        // сигнатуры всех её функций — панель ошибок удваивается как подсказка
        // по API. Имена параметров рендерер теряет (типы остаются).
        let diags = ulua::check_with_definitions(
            "return function(frame: Frame) host.firr(frame[1]) end",
            defs,
        )
        .unwrap_err();
        assert!(diags.iter().any(|d| {
            d.message
                .contains("fir: (Channel, number, number, number) -> ()")
        }));

        // Инвариант 2: незавершённый вызов во время набора уже даёт
        // arity-подсказку — параллельно с ошибкой парсинга, без паники.
        let diags = ulua::check_with_definitions("return function(frame: Frame) host.fir(", defs)
            .unwrap_err();
        assert!(
            diags
                .iter()
                .any(|d| d.message.contains("expects 4 arguments"))
        );

        // Стоимость одного вызова (front-end + builtins + defs парсятся каждый
        // раз — переиспользуемый `Checker` definitions не поддерживает):
        // ориентир для debounce в виджете, включая wasm.
        let started = std::time::Instant::now();
        for _ in 0..10 {
            let _ = ulua::check_with_definitions(clean, defs);
        }
        println!(
            "check_with_definitions: {:.1} ms/call",
            started.elapsed().as_secs_f64() * 100.0 / 10.0
        );
    }
}
