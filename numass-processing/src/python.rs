//! Python-биндинги конфигураций. Собирается только с фичей `python`.
//!
//! Классы — это сами типы `numass-processing`. Здесь только модуль,
//! конструкторы с дефолтами и фабрики без аргументов.

use pyo3::prelude::*;
use pyo3::types::PyType;

use crate::postprocess::{ChannelIgnore, PostProcessParams};
use crate::preprocess::PreprocessParams;
use crate::process::{Algorithm, HWResetParams, HistParams, ProcessParams};

#[cfg(feature = "stubgen")]
pyo3_stub_gen::module_doc!(
    "numass_processing",
    "Конфигурационные объекты numass-processing."
);

#[cfg(feature = "stubgen")]
pyo3_stub_gen::define_stub_info_gatherer!(stub_info);

/// Конфигурационные объекты numass-processing.
///
/// Query string и REST (`HistParams::to_query_string`) сюда не входят.
#[pymodule]
fn numass_processing(m: &Bound<'_, PyModule>) -> PyResult<()> {
    use crate::preprocess::{Baseline, PreprocessStep};
    use crate::process::SkipOption;

    m.add_class::<Baseline>()?;
    m.add_class::<PreprocessStep>()?;
    m.add_class::<PreprocessParams>()?;
    m.add_class::<SkipOption>()?;
    m.add_class::<HWResetParams>()?;
    m.add_class::<Algorithm>()?;
    m.add_class::<ProcessParams>()?;
    m.add_class::<ChannelIgnore>()?;
    m.add_class::<PostProcessParams>()?;
    m.add_class::<HistParams>()?;
    Ok(())
}

#[cfg_attr(feature = "stubgen", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[pymethods]
impl PreprocessParams {
    /// Цепочка шагов. Без аргументов — [`PreprocessParams::default`] (`average`).
    #[new]
    #[pyo3(signature = (steps = PreprocessParams::default().steps))]
    fn new(steps: Vec<crate::preprocess::PreprocessStep>) -> Self {
        Self { steps }
    }

    /// Пустая цепочка: кадр проходит без предобработки.
    #[classmethod]
    #[pyo3(name = "empty")]
    fn py_empty(_cls: &Bound<'_, PyType>) -> Self {
        Self::empty()
    }

    /// FIR + пик по точке. Сохраняет числа прежнего Trapezoid.
    #[classmethod]
    #[pyo3(name = "trapezoid")]
    fn py_trapezoid(_cls: &Bound<'_, PyType>) -> Self {
        Self::trapezoid()
    }

    /// Только FIR, без пика по точке. Для одного живого кадра.
    #[classmethod]
    #[pyo3(name = "trapezoid_fir")]
    fn py_trapezoid_fir(_cls: &Bound<'_, PyType>) -> Self {
        Self::trapezoid_fir()
    }

    /// Голова кадра + инверсия. Сохраняет числа прежнего Average.
    #[classmethod]
    #[pyo3(name = "average")]
    fn py_average(_cls: &Bound<'_, PyType>) -> Self {
        Self::average()
    }
}

#[cfg_attr(feature = "stubgen", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[pymethods]
impl HWResetParams {
    #[new]
    #[pyo3(signature = (
        window = HWResetParams::default().window,
        treshold = HWResetParams::default().treshold,
        size = HWResetParams::default().size,
    ))]
    fn new(window: u16, treshold: i16, size: u16) -> Self {
        Self {
            window,
            treshold,
            size,
        }
    }
}

#[cfg_attr(feature = "stubgen", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[pymethods]
impl ProcessParams {
    #[new]
    #[pyo3(signature = (
        algorithm = ProcessParams::default().algorithm,
        convert_to_kev = ProcessParams::default().convert_to_kev,
    ))]
    fn new(algorithm: Algorithm, convert_to_kev: bool) -> Self {
        Self {
            algorithm,
            convert_to_kev,
        }
    }
}

#[cfg_attr(feature = "stubgen", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[pymethods]
impl ChannelIgnore {
    #[new]
    #[pyo3(signature = (
        ch1 = ChannelIgnore::default().ch1,
        ch2 = ChannelIgnore::default().ch2,
        ch3 = ChannelIgnore::default().ch3,
        ch4 = ChannelIgnore::default().ch4,
        ch5 = ChannelIgnore::default().ch5,
        ch6 = ChannelIgnore::default().ch6,
        ch7 = ChannelIgnore::default().ch7,
    ))]
    fn new(ch1: bool, ch2: bool, ch3: bool, ch4: bool, ch5: bool, ch6: bool, ch7: bool) -> Self {
        Self {
            ch1,
            ch2,
            ch3,
            ch4,
            ch5,
            ch6,
            ch7,
        }
    }
}

#[cfg_attr(feature = "stubgen", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[pymethods]
impl PostProcessParams {
    #[new]
    #[pyo3(signature = (
        cut_bad_blocks = PostProcessParams::default().cut_bad_blocks,
        merge_frames = PostProcessParams::default().merge_frames,
        merge_splits_first = PostProcessParams::default().merge_splits_first,
        merge_close_events = PostProcessParams::default().merge_close_events,
        ignore_borders = PostProcessParams::default().ignore_borders,
        ignore_channels = PostProcessParams::default().ignore_channels,
    ))]
    fn new(
        cut_bad_blocks: bool,
        merge_frames: Option<u16>,
        merge_splits_first: bool,
        merge_close_events: bool,
        ignore_borders: bool,
        ignore_channels: ChannelIgnore,
    ) -> Self {
        Self {
            cut_bad_blocks,
            merge_frames,
            merge_splits_first,
            merge_close_events,
            ignore_borders,
            ignore_channels,
        }
    }
}

#[cfg_attr(feature = "stubgen", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[pymethods]
impl HistParams {
    #[new]
    #[pyo3(signature = (
        preprocess = HistParams::default().preprocess,
        process = HistParams::default().process,
        postprocess = HistParams::default().postprocess,
    ))]
    fn new(
        preprocess: PreprocessParams,
        process: ProcessParams,
        postprocess: Option<PostProcessParams>,
    ) -> Self {
        Self {
            preprocess,
            process,
            postprocess,
        }
    }
}
