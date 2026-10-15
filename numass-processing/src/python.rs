//! Python-биндинги. Собирается только с фичей `python`.
//!
//! Конфиги — сами типы `numass-processing`. Шаги обработки вызывают те же
//! функции Rust и возвращают Rust-структуру. Промежуточные данные не копируются:
//! [`Waveforms`] и [`Frame`] держат [`Point`]. `to_polars` — односторонний выход.

use std::path::PathBuf;

use numass::protos::rsb_event;
use pyo3::exceptions::{PyFileNotFoundError, PyKeyError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyType};
use pyo3_polars::PyDataFrame;

use crate::postprocess::{ChannelIgnore, PostProcessParams};
use crate::preprocess::PreprocessParams;
use crate::process::{Algorithm, HWResetParams, HistParams, ProcessParams};

#[cfg(feature = "stubgen")]
pyo3_stub_gen::module_doc!(
    "numass_processing",
    "Шаги обработки numass один в один с Rust. to_polars() — односторонний выход в таблицу Polars."
);

#[cfg(feature = "stubgen")]
pyo3_stub_gen::define_stub_info_gatherer!(stub_info);

/// Шаги обработки numass один в один с Rust.
///
/// Query string и REST (`HistParams::to_query_string`) сюда не входят.
/// После `to_polars()` методы этого модуля уже не вызываются.
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

    m.add_class::<Point>()?;
    m.add_class::<Waveforms>()?;
    m.add_class::<Frame>()?;
    m.add_class::<ProcessedPoint>()?;
    m.add_class::<ProcessedFrame>()?;
    m.add_class::<NumassEvents>()?;
    m.add_class::<FrameEvents>()?;

    #[cfg(not(target_arch = "wasm32"))]
    m.add_function(wrap_pyfunction!(load_point, m)?)?;
    #[cfg(not(target_arch = "wasm32"))]
    m.add_function(wrap_pyfunction!(load_meta, m)?)?;
    m.add_function(wrap_pyfunction!(extract_waveforms, m)?)?;
    m.add_function(wrap_pyfunction!(apply_waveforms, m)?)?;
    m.add_function(wrap_pyfunction!(apply_frame, m)?)?;
    m.add_function(wrap_pyfunction!(extract_processed, m)?)?;
    m.add_function(wrap_pyfunction!(extract_frame, m)?)?;
    m.add_function(wrap_pyfunction!(convert_to_kev, m)?)?;
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

fn polars_table(frame: polars::prelude::DataFrame) -> PyDataFrame {
    PyDataFrame(frame)
}

fn serde_error(err: serde_json::Error) -> PyErr {
    PyValueError::new_err(err.to_string())
}

/// `serde_json::Value` → объект Python. Ошибка serde — если значение не сериализуется.
fn json_to_py(py: Python<'_>, value: &serde_json::Value) -> PyResult<Py<PyAny>> {
    let text = serde_json::to_string(value).map_err(serde_error)?;
    Ok(py.import("json")?.call_method1("loads", (text,))?.unbind())
}

/// Словарь ↔ [`numass::NumassMeta`]. Ошибка serde с любого направления уходит в Python.
fn read_meta_dict(py: Python<'_>, dict: &Bound<'_, PyDict>) -> PyResult<()> {
    let text: String = py
        .import("json")?
        .call_method1("dumps", (dict,))?
        .extract()?;
    let meta: numass::NumassMeta = serde_json::from_str(&text).map_err(serde_error)?;
    serde_json::to_value(&meta).map_err(serde_error)?;
    Ok(())
}

/// Протокольная точка. Данные кадров лежат здесь; следующие шаги только ссылаются на них.
#[cfg_attr(feature = "stubgen", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(module = "numass_processing")]
struct Point {
    inner: rsb_event::Point,
}

/// Кадры точки без копии отсчётов. Время кадра — ключ, как в `extract_waveforms`.
#[cfg_attr(feature = "stubgen", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(module = "numass_processing")]
struct Waveforms {
    point: Py<Point>,
}

/// Один кадр точки. Отсчёты остаются в [`Point`].
#[cfg_attr(feature = "stubgen", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(module = "numass_processing")]
struct Frame {
    point: Py<Point>,
    time: u64,
}

/// Точка после [`apply_waveforms`].
#[cfg_attr(feature = "stubgen", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(module = "numass_processing")]
struct ProcessedPoint {
    inner: crate::preprocess::ProcessedPoint,
}

/// Кадр после [`apply_frame`].
#[cfg_attr(feature = "stubgen", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(module = "numass_processing")]
struct ProcessedFrame {
    inner: crate::preprocess::ProcessedFrame,
}

/// События точки после [`extract_processed`].
#[cfg_attr(feature = "stubgen", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(module = "numass_processing")]
struct NumassEvents {
    inner: crate::types::NumassEvents,
}

/// События одного кадра после [`extract_frame`].
#[cfg_attr(feature = "stubgen", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[pyclass(module = "numass_processing")]
struct FrameEvents {
    inner: Vec<crate::types::NumassEvent>,
}

fn frame_at(point: &rsb_event::Point, time: u64) -> Option<crate::types::NumassFrameFast<'_>> {
    crate::preprocess::extract_waveforms(point)
        .get(&time)
        .cloned()
}

#[cfg_attr(feature = "stubgen", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[cfg_attr(not(feature = "stubgen"), pyo3_stub_gen_derive::remove_gen_stub)]
#[pymethods]
impl Waveforms {
    /// Кадр с этим временем (нс от начала набора). Отсчёты не копируются.
    fn __getitem__(&self, py: Python<'_>, time: u64) -> PyResult<Frame> {
        let point = self.point.borrow(py);
        if frame_at(&point.inner, time).is_none() {
            return Err(PyKeyError::new_err(time));
        }
        Ok(Frame {
            point: self.point.clone_ref(py),
            time,
        })
    }

    /// Односторонняя таблица сырых кадров: `time`, `channel`, `index`, `value`.
    #[gen_stub(override_return_type(type_repr = "polars.DataFrame", imports = ("polars")))]
    fn to_polars(&self, py: Python<'_>) -> PyDataFrame {
        let point = self.point.borrow(py);
        let waveforms = crate::preprocess::extract_waveforms(&point.inner);
        polars_table(crate::to_polars::ToPolars::to_polars(&waveforms))
    }
}

#[cfg_attr(feature = "stubgen", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[cfg_attr(not(feature = "stubgen"), pyo3_stub_gen_derive::remove_gen_stub)]
#[pymethods]
impl Frame {
    /// Односторонняя таблица кадра: `channel`, `index`, `value`.
    #[gen_stub(override_return_type(type_repr = "polars.DataFrame", imports = ("polars")))]
    fn to_polars(&self, py: Python<'_>) -> PyResult<PyDataFrame> {
        let point = self.point.borrow(py);
        let Some(frame) = frame_at(&point.inner, self.time) else {
            return Err(PyKeyError::new_err(self.time));
        };
        Ok(polars_table(crate::to_polars::ToPolars::to_polars(&frame)))
    }
}

#[cfg_attr(feature = "stubgen", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[cfg_attr(not(feature = "stubgen"), pyo3_stub_gen_derive::remove_gen_stub)]
#[pymethods]
impl ProcessedPoint {
    /// Односторонняя таблица: одна строка на осциллограмму канала.
    /// `time`, `channel`, `delay`, `value` (вектор значений бинов).
    #[gen_stub(override_return_type(type_repr = "polars.DataFrame", imports = ("polars")))]
    fn to_polars(&self) -> PyDataFrame {
        polars_table(crate::to_polars::ToPolars::to_polars(&self.inner))
    }
}

#[cfg_attr(feature = "stubgen", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[cfg_attr(not(feature = "stubgen"), pyo3_stub_gen_derive::remove_gen_stub)]
#[pymethods]
impl ProcessedFrame {
    /// Односторонняя таблица: `channel`, `index`, `delay`, `value`.
    #[gen_stub(override_return_type(type_repr = "polars.DataFrame", imports = ("polars")))]
    fn to_polars(&self) -> PyDataFrame {
        polars_table(crate::to_polars::ToPolars::to_polars(&self.inner))
    }
}

#[cfg_attr(feature = "stubgen", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[cfg_attr(not(feature = "stubgen"), pyo3_stub_gen_derive::remove_gen_stub)]
#[pymethods]
impl NumassEvents {
    /// Односторонняя таблица: `time`, `position`, `kind`, `channel`, `amplitude`, `size`.
    #[gen_stub(override_return_type(type_repr = "polars.DataFrame", imports = ("polars")))]
    fn to_polars(&self) -> PyDataFrame {
        polars_table(crate::to_polars::ToPolars::to_polars(&self.inner))
    }
}

#[cfg_attr(feature = "stubgen", pyo3_stub_gen::derive::gen_stub_pymethods)]
#[cfg_attr(not(feature = "stubgen"), pyo3_stub_gen_derive::remove_gen_stub)]
#[pymethods]
impl FrameEvents {
    /// Односторонняя таблица: `position`, `kind`, `channel`, `amplitude`, `size`.
    #[gen_stub(override_return_type(type_repr = "polars.DataFrame", imports = ("polars")))]
    fn to_polars(&self) -> PyDataFrame {
        polars_table(crate::to_polars::ToPolars::to_polars(&self.inner))
    }
}

/// Загружает протокольную точку из файла DataForge.
#[cfg(not(target_arch = "wasm32"))]
#[cfg_attr(feature = "stubgen", pyo3_stub_gen::derive::gen_stub_pyfunction)]
#[pyfunction]
fn load_point(path: PathBuf) -> PyResult<Point> {
    if !path.is_file() {
        return Err(PyFileNotFoundError::new_err(path.display().to_string()));
    }
    Ok(Point {
        inner: crate::storage::load_point_sync(&path),
    })
}

/// Полный JSON метаданных точки из DataForge, без разбора в `NumassMeta`.
#[cfg(not(target_arch = "wasm32"))]
#[cfg_attr(feature = "stubgen", pyo3_stub_gen::derive::gen_stub_pyfunction)]
#[cfg_attr(not(feature = "stubgen"), pyo3_stub_gen_derive::remove_gen_stub)]
#[pyfunction]
#[gen_stub(override_return_type(type_repr = "dict[str, typing.Any]", imports = ("typing")))]
fn load_meta(py: Python<'_>, path: PathBuf) -> PyResult<Py<PyAny>> {
    if !path.is_file() {
        return Err(PyFileNotFoundError::new_err(path.display().to_string()));
    }
    let value = crate::storage::load_meta_json_sync(&path)
        .map_err(|err| PyValueError::new_err(err.to_string()))?;
    json_to_py(py, &value)
}

/// Кадры точки без копии отсчётов. `point` остаётся владельцем данных.
#[cfg_attr(feature = "stubgen", pyo3_stub_gen::derive::gen_stub_pyfunction)]
#[pyfunction]
fn extract_waveforms(point: Py<Point>) -> Waveforms {
    Waveforms { point }
}

/// Те же шаги, что `preprocess::apply_waveforms`.
#[cfg_attr(feature = "stubgen", pyo3_stub_gen::derive::gen_stub_pyfunction)]
#[pyfunction]
fn apply_waveforms(
    py: Python<'_>,
    waveforms: &Waveforms,
    steps: Vec<crate::preprocess::PreprocessStep>,
) -> ProcessedPoint {
    let point = waveforms.point.borrow(py);
    let fast = crate::preprocess::extract_waveforms(&point.inner);
    ProcessedPoint {
        inner: crate::preprocess::apply_waveforms(&fast, &steps),
    }
}

/// Те же шаги, что `preprocess::apply_frame`. `Baseline::Point` видит только этот кадр.
#[cfg_attr(feature = "stubgen", pyo3_stub_gen::derive::gen_stub_pyfunction)]
#[pyfunction]
fn apply_frame(
    py: Python<'_>,
    frame: &Frame,
    steps: Vec<crate::preprocess::PreprocessStep>,
) -> PyResult<ProcessedFrame> {
    let point = frame.point.borrow(py);
    let Some(fast) = frame_at(&point.inner, frame.time) else {
        return Err(PyKeyError::new_err(frame.time));
    };
    Ok(ProcessedFrame {
        inner: crate::preprocess::apply_frame(&fast, &steps),
    })
}

/// События точки. `processed` и `waveforms` — одна точка после предобработки.
///
/// Тот же проход, что `process::extract_processed`: `extract_frame` по каждому
/// кадру и перевод амплитуды в кэВ, если `process.convert_to_kev`.
/// `meta` — словарь JSON точки; если он передан, читается как `NumassMeta`.
#[cfg_attr(feature = "stubgen", pyo3_stub_gen::derive::gen_stub_pyfunction)]
#[cfg_attr(not(feature = "stubgen"), pyo3_stub_gen_derive::remove_gen_stub)]
#[pyfunction]
#[pyo3(signature = (processed, waveforms, process, meta = None))]
fn extract_processed(
    py: Python<'_>,
    processed: &ProcessedPoint,
    waveforms: &Waveforms,
    process: ProcessParams,
    #[gen_stub(override_type(
        type_repr = "dict[str, typing.Any] | None",
        imports = ("typing")
    ))]
    meta: Option<Py<PyDict>>,
) -> PyResult<NumassEvents> {
    // Словарь нужен только как NumassMeta: схема проверяется здесь, события от неё не зависят.
    if let Some(meta) = meta {
        read_meta_dict(py, meta.bind(py))?;
    }
    let point = waveforms.point.borrow(py);
    let raw = crate::preprocess::extract_waveforms(&point.inner);
    Ok(NumassEvents {
        inner: crate::process::extract_processed(&processed.inner, &raw, &process),
    })
}

/// Те же аргументы, что `process::extract_frame`, без egui.
#[cfg_attr(feature = "stubgen", pyo3_stub_gen::derive::gen_stub_pyfunction)]
#[pyfunction]
fn extract_frame(
    py: Python<'_>,
    processed: &ProcessedFrame,
    frame: &Frame,
    algorithm: Algorithm,
) -> PyResult<FrameEvents> {
    let point = frame.point.borrow(py);
    let Some(raw) = frame_at(&point.inner, frame.time) else {
        return Err(PyKeyError::new_err(frame.time));
    };
    Ok(FrameEvents {
        inner: crate::process::extract_frame(
            &processed.inner,
            &raw,
            &algorithm,
            #[cfg(feature = "egui")]
            &mut None,
        ),
    })
}

/// Те же аргументы, что `process::convert_to_kev`.
#[cfg_attr(feature = "stubgen", pyo3_stub_gen::derive::gen_stub_pyfunction)]
#[pyfunction]
fn convert_to_kev(amplitude: f32, ch_id: u8, algorithm: Algorithm) -> f32 {
    crate::process::convert_to_kev(&amplitude, ch_id, &algorithm)
}
