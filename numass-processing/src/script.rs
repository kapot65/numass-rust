//! Скриптовый шаг предобработки на [ulua](https://github.com/webc-site/ulua) (Luau).
//!
//! Скрипт получает кадр таблицей Luau и возвращает обработанные отсчёты.
//! Обмен значениями идёт через таблицы, которые переиспользуются между
//! кадрами, а числовые циклы выполняются внутри VM.
//!
//! [`StaticPointParams`] передаётся глобалом `point`: таблица собирается
//! один раз на прогон точки.
//!
//! Функции хоста (FFT, отрисовка графиков и другие операции, которые не
//! выражаются циклом Luau) регистрируются при сборке VM в таблицу `host`.
//! VM создаётся для каждого вызова шага в потоке, выполнившем этот вызов.

use std::cell::RefCell;

use ulua::rt::LuaSerdeExt;
use ulua::{Compiler, Function, IntoLua, Lua, Table};

use crate::preprocess::{ProcessedFrame, ProcessedPoint, StaticPointParams};

pub type ScriptError = ulua::Error;

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
    fn install(&self, _lua: &Lua, _host: &Table) -> Result<(), ScriptError> {
        // FFT и отрисовка регистрируются здесь, когда появятся.
        Ok(())
    }
}

/// Таблицы одного канала, переиспользуемые между кадрами.
///
/// Длина кадра в точке постоянна, поэтому массив отсчётов выделяется один
/// раз, а на каждом кадре перезаписываются только числа.
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
    /// Таблицы первого кадра. Между кадрами не пересоздаются.
    scratch: RefCell<Option<(Table, Vec<ChannelScratch>)>>,
}

impl ScriptRuntime {
    /// Компилирует `source` на максимальном уровне оптимизации Luau.
    ///
    /// Скрипт обязан вернуть `function(frame)`. До вызова хост выставляет
    /// глобалы:
    /// - `point` — таблица из [`StaticPointParams`] или `nil`;
    /// - `host` — таблица функций [`ScriptHost`].
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
        lua.globals().set("host", host_table)?;

        let process: Function = lua
            .load(source)
            .set_name("preprocess")
            .set_compiler(compiler)
            .eval()?;

        Ok(Self {
            lua,
            process,
            scratch: RefCell::new(None),
        })
    }

    /// Выполняет скрипт для каждого кадра точки.
    ///
    /// Таблица `point` собирается один раз на вызов. Отсчёты кадра
    /// записываются в таблицы рантайма и после вызова читаются обратно.
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
                let samples = self
                    .lua
                    .create_table_with_capacity(channel.samples.len(), 0);
                let body = self.lua.create_table_with_capacity(0, 2);
                body.raw_set("samples", samples.clone())?;
                table.raw_set(i64::from(*id), body.clone())?;
                channels.push(ChannelScratch { body, samples });
            }
        }

        for (slot, (_, channel)) in channels.iter().zip(frame) {
            slot.body.raw_set("delay", channel.delay)?;
            let len = channel.samples.len();
            // Хвост более длинной таблицы срезается, чтобы `#samples`
            // совпадал с длиной кадра.
            while slot.samples.raw_len() > len {
                slot.samples.raw_pop::<ulua::Value>()?;
            }
            for (index, sample) in channel.samples.iter().enumerate() {
                slot.samples.raw_set(index + 1, f64::from(*sample))?;
            }
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
            let len = slot.samples.raw_len();
            channel.samples.clear();
            channel.samples.reserve(len);
            for value in slot.samples.sequence_values::<f64>() {
                channel.samples.push(value? as f32);
            }
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
}
