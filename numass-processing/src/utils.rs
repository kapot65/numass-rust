//! # Utils
//! This module contains some utility functions not related to processing or postprocessing.

use std::path::PathBuf;

#[cfg(feature = "plotly")]
use plotly::color::Color;
#[cfg(feature = "egui")]
use {
    crate::types::{ProcessedWaveform, RawWaveform},
    egui::{Color32, epaint::Hsva},
    egui_plot::{HoverPosition, Line, PlotPoint, PlotUi},
};

use crate::{
    histogram::{HistogramParams, PointHistogram},
    types::{
        FrameEvent, NumassEvents, Time, microsecond, millisecond, nanosecond, nanoseconds, second,
        time_ns,
    },
};
use numass::{NumassMeta, Reply};
use uom::si::Unit;

pub fn events_to_histogram(amplitudes: NumassEvents, histogram: HistogramParams) -> PointHistogram {
    let mut histogram = PointHistogram::from(histogram);

    for (_, channels) in amplitudes {
        for (_, event) in channels {
            if let FrameEvent::Event {
                channel, amplitude, ..
            } = event
            {
                histogram.add(channel, amplitude)
            }
        }
    }

    histogram
}

/// Convert path in db (RUN/FILL/SET/POINT) to filename with info preserved.
pub fn construct_filename(name: &str, pref_ext: Option<&str>) -> String {
    let temp = PathBuf::from(name);

    let mut name = temp.file_name().unwrap().to_str().unwrap().to_owned();
    if let Some(pref_ext) = pref_ext
        && !name.ends_with(pref_ext)
    {
        name = format!("{name}.{pref_ext}");
    }

    let mut set = "".to_string();
    let mut run = "".to_string();

    if let Some(set_path) = temp.parent()
        && let Some(set_filename) = set_path.file_name()
        && let Some(set_str) = set_filename.to_str()
        && let Some(run_path) = set_path.parent()
        && let Some(run_filename) = run_path.file_name()
        && let Some(run_str) = run_filename.to_str()
    {
        set = set_str.to_string();
        run = run_str.to_string();
    }

    format!("{run}{set}{name}")
}

/// Форматирует время с автоматическим подбором SI-приставки
/// (ns, µs, ms, s) — единый вид всех временных подписей на графиках.
///
/// Число и обозначение единицы — из представления uom: пересчёт —
/// [`Time::get`], обозначение — [`uom::si::Unit::abbreviation`].
/// Незначащие нули отбрасываются: `4 µs`, `180 ms`.
pub fn format_time(value: Time) -> String {
    // округляем до целых наносекунд: конверсии uom (в секунды и обратно)
    // вносят float-погрешность, и значение ровно на границе приставки
    // (например 1e6 нс) может уехать чуть ниже и выбрать неверную приставку
    let ns = value.get::<nanosecond>().round();
    let (scaled, unit) = if ns.abs() < 1e3 {
        (ns, nanosecond::abbreviation())
    } else if ns.abs() < 1e6 {
        (value.get::<microsecond>(), microsecond::abbreviation())
    } else if ns.abs() < 1e9 {
        (value.get::<millisecond>(), millisecond::abbreviation())
    } else {
        (value.get::<second>(), second::abbreviation())
    };

    let mut text = format!("{scaled:.3}");
    while text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    if text == "-0" {
        text = "0".to_owned();
    }

    format!("{text} {unit}")
}

#[cfg(feature = "egui")]
fn format_plot_number(number: f64) -> String {
    if number.is_finite() && number.fract() == 0.0 && number.abs() < 1e15 {
        format!("{number:.0}")
    } else {
        format!("{number:.3}")
    }
}

/// Подпись точки при наведении: сырые координаты графика и,
/// если задан форматтер, то же значение в единицах оси.
///
/// С egui_plot 0.37 подпись сама не появляется — её надо передать в
/// [`egui_plot::Plot::label_formatter`]. `None` в форматтере оставляет ось
/// только сырой (амплитуда осциллограмм).
#[cfg(feature = "egui")]
pub fn plot_hover_label(
    pos: &HoverPosition<'_>,
    format_x: impl Fn(f64) -> Option<String>,
    format_y: impl Fn(f64) -> Option<String>,
) -> Option<String> {
    let (name, PlotPoint { x, y }) = match *pos {
        HoverPosition::NearDataPoint {
            plot_name,
            position,
            ..
        } => (plot_name, position),
        HoverPosition::Elsewhere { position } => ("", position),
    };

    let mut lines = Vec::new();
    if !name.is_empty() {
        lines.push(name.to_owned());
    }
    lines.push(axis_hover_line("x", x, format_x(x)));
    lines.push(axis_hover_line("y", y, format_y(y)));
    Some(lines.join("\n"))
}

#[cfg(feature = "egui")]
fn axis_hover_line(axis: &str, raw: f64, formatted: Option<String>) -> String {
    match formatted {
        Some(formatted) => format!("{axis} = {}  ({formatted})", format_plot_number(raw)),
        None => format!("{axis} = {}", format_plot_number(raw)),
    }
}

/// Подпись осциллограммы: сырое x — номер бина АЦП
/// (обратное к [`crate::types::bin_time`]: нс / [`TICK_NS`]),
/// рядом — то же время с подбором приставки.
/// y — амплитуда как нарисована (ADC или keV).
#[cfg(feature = "egui")]
pub fn waveform_hover_label(pos: &HoverPosition<'_>) -> Option<String> {
    let (name, point) = match *pos {
        HoverPosition::NearDataPoint {
            plot_name,
            position,
            ..
        } => (plot_name, position),
        HoverPosition::Elsewhere { position } => ("", position),
    };

    let mut lines = Vec::new();
    if !name.is_empty() {
        lines.push(name.to_owned());
    }
    // сырое x — бин АЦП, не наносекунды оси: время уже стоит рядом в скобках
    lines.push(axis_hover_line(
        "x",
        waveform_bin(point.x) as f64,
        Some(format_time(nanoseconds(point.x))),
    ));
    lines.push(axis_hover_line("y", point.y, None));
    Some(lines.join("\n"))
}

/// Номер бина АЦП по координате оси x (наносекунды) — обратное к
/// [`crate::types::bin_time`]. Округляем: точка лежит на сетке бина,
/// но перевод через секунды uom мог её чуть сдвинуть.
#[cfg(feature = "egui")]
fn waveform_bin(x_ns: f64) -> i64 {
    (x_ns / f64::from(crate::types::TICK_NS)).round() as i64
}

/// Корретировка времени прихода триггера
///
/// Для некоторых точек начиная с определенного триггера ко времени примешивается какое-то огромное число
/// Это происходит как минимум с сеанса 2024_03
/// Судя по всему это константная величина
/// TODO: найти точное значение
pub fn correct_frame_time(time: u64) -> u64 {
    if time > 0xf000_0000_0000_0000 {
        time - 0xffff_fff9_03da_0000
    } else {
        time
    }
}

/// Время набора точки из метаданных, если ответ `acquire_point` есть.
///
/// В метаданных оно хранится в секундах.
pub fn acquisition_time(meta: Option<NumassMeta>) -> Option<Time> {
    match meta {
        Some(NumassMeta::Reply(Reply::AcquirePoint {
            acquisition_time, ..
        })) => Some(Time::new::<second>(f64::from(acquisition_time))),
        _ => None,
    }
}

/// Верхняя граница валидных таймстампов кадра в наносекундах: конец набора
/// плюс секундный запас на ход часов.
///
/// Таймстампы дальше — мусорные: к ним примешана константа, которую не знает
/// [`correct_frame_time`]. Без границы такие кадры раздувают число чанков
/// (индекс = `таймстамп / limit`) до capacity overflow.
pub fn timestamps_bound(acquisition_time: Time) -> u64 {
    time_ns(acquisition_time + Time::new::<second>(1.0))
}

/// Результат [`repair_frame_times`].
#[derive(Debug)]
pub struct RepairedTimes {
    /// Положение каждого кадра от начала точки в наносекундах или `None`
    /// для безнадёжного мусора.
    pub times: Vec<Option<u64>>,
    /// Сколько таймстампов поправлено (сдвинуто на поправку).
    pub repaired: usize,
    /// Постоянный шаг в наносекундах, вычисаемый из всех таймстампов
    /// сегмента сбоя; по одному на сегмент — каждый следующий сегмент
    /// заякоривается за концом предыдущей шкалы.
    pub shifts_ns: Vec<u64>,
}

/// «Ремонтирует» времена кадров, к которым примешалась неизвестная константа,
/// вместо того чтобы их отбрасывать.
///
/// Модель сбоя — как в [`correct_frame_time`]: начиная с определенного
/// триггера к времени кадров прибавляется константа, и кадры продолжают
/// идти с обычными интервалами. Поэтому кадры в пределах `bound` от начала
/// шкалы считаются валидными и идут как есть, а остальные образуют сегменты
/// (кадры с интервалами не больше `bound`), и каждый сегмент ставится сразу
/// за концом предыдущей шкалы — внутренние интервалы сохраняются. Сегмент
/// длиннее `bound` — не сдвиг константой, а мусор: такой хвост отбрасывается.
pub fn repair_frame_times(times: &[u64], bound: u64) -> RepairedTimes {
    let Some(t0) = times.iter().min().copied() else {
        return RepairedTimes {
            times: Vec::new(),
            repaired: 0,
            shifts_ns: Vec::new(),
        };
    };

    let mut repaired = vec![None; times.len()];

    // Валидная шкала — кадры в пределах bound от начала.
    let mut timeline_end = 0;
    for (slot, &time) in repaired.iter_mut().zip(times) {
        let relative = time - t0;
        if relative <= bound {
            *slot = Some(relative);
            timeline_end = timeline_end.max(relative);
        }
    }

    // Сломанные кадры по возрастанию времени.
    let mut broken: Vec<(usize, u64)> = times
        .iter()
        .enumerate()
        .filter_map(|(idx, &time)| (time - t0 > bound).then_some((idx, time - t0)))
        .collect();
    broken.sort_unstable_by_key(|&(_, time)| time);

    let Some(&(_, first_time)) = broken.first() else {
        return RepairedTimes {
            times: repaired,
            repaired: 0,
            shifts_ns: Vec::new(),
        };
    };

    let mut repaired_count = 0;
    let mut shifts = Vec::new();

    let mut anchor = timeline_end;
    let mut segment_start = first_time;
    let mut prev = first_time;
    for &(idx, time) in &broken {
        // Разрыв больше bound — начался новый сегмент сбоя: якорь — конец
        // уже набранной шкалы.
        if time - prev > bound {
            segment_start = time;
            anchor = timeline_end;
        }
        prev = time;

        if time - segment_start > bound {
            continue; // не сдвиг константой, а мусор внутри сегмента
        }

        let placed = anchor + (time - segment_start);
        repaired[idx] = Some(placed);
        repaired_count += 1;
        // поправка внутри сегмента постоянна (сдвиг его первого кадра);
        // разные сегменты могут совпадать только случайно — не раздуваем
        if shifts.last() != Some(&(segment_start - anchor)) {
            shifts.push(segment_start - anchor);
        }
        timeline_end = timeline_end.max(placed);
    }

    RepairedTimes {
        times: repaired,
        repaired: repaired_count,
        shifts_ns: shifts,
    }
}

/// Parabolic event amplitude correction correction
pub fn correct_amp(y0: f32, y1: f32, y2: f32) -> (f32, f32) {
    (
        // calculated with SymPy
        (y0 - y2) / (2.0 * (y0 - 2.0 * y1 + y2)),
        (-(y0 * y0) / 8.0 + y0 * y1 + y0 * y2 / 4.0 - 2.0 * y1 * y1 + y1 * y2 - (y2 * y2) / 8.0)
            / (y0 - 2.0 * y1 + y2),
    )
}

#[cfg(feature = "egui")]
/// Returns color for channel index
/// Color will be same as [color_for_index_str](color_for_index_str)
pub fn color_for_index(idx: usize) -> Color32 {
    let golden_ratio = (5.0_f32.sqrt() - 1.0) / 2.0; // 0.61803398875
    let h = idx as f32 * golden_ratio;
    Hsva::new(h, 0.85, 0.5, 1.0).into()
}

#[cfg(feature = "plotly")]
/// Returns color for channel index
/// Color will be same as [color_for_index](color_for_index)
pub fn color_for_index_str(idx: usize) -> impl Color {
    let golden_ratio = (5.0_f32.sqrt() - 1.0) / 2.0; // 0.61803398875
    let h = idx as f32 * golden_ratio;

    let (r, g, b) = rgb_hsv::hsv_to_rgb((h, 0.85, 0.66));
    format!("rgb({r}, {g}, {b})")
}

#[cfg(feature = "egui")]
pub trait EguiLine: Into<Vec<[f64; 2]>> {
    /// Рисует вейвформу линией. Ось x — время от начала кадра;
    /// `offset` сдвигает все точки по времени.
    fn draw_egui(
        self,
        plot_ui: &mut PlotUi,
        name: Option<&str>,
        color: Option<Color32>,
        thickness: Option<f32>,
        offset: Option<Time>,
    ) {
        let mut points: Vec<[f64; 2]> = self.into();
        if let Some(offset) = offset {
            points
                .iter_mut()
                .for_each(|[x, _]| *x += offset.get::<nanosecond>())
        }

        let mut line = Line::new(name.unwrap_or_default(), points);
        if let Some(color) = color {
            line = line.color(color)
        }
        if let Some(thickness) = thickness {
            line = line.width(thickness)
        }

        plot_ui.line(line);
    }
}

#[cfg(feature = "egui")]
impl EguiLine for RawWaveform {}

#[cfg(feature = "egui")]
impl EguiLine for ProcessedWaveform {}

#[cfg(test)]
mod tests {
    use super::{format_time, repair_frame_times};
    use crate::types::{TICK_NS, Time, bin_time, nanoseconds, second, time_ns};

    #[test]
    fn format_time_picks_si_prefix() {
        assert_eq!(format_time(nanoseconds(500.0)), "500 ns");
        assert_eq!(format_time(nanoseconds(4000.0)), "4 µs");
        assert_eq!(format_time(nanoseconds(-3125.0)), "-3.125 µs");
        assert_eq!(format_time(nanoseconds(1.8e8)), "180 ms");
        assert_eq!(format_time(Time::new::<second>(30.0)), "30 s");
    }

    #[test]
    fn format_time_uses_next_prefix_at_exact_boundary() {
        // 1e3/1e6/1e9 нс после конверсий uom могут уехать чуть ниже границы
        assert_eq!(format_time(nanoseconds(1e3)), "1 µs");
        assert_eq!(format_time(nanoseconds(1e6)), "1 ms");
        assert_eq!(format_time(nanoseconds(1e9)), "1 s");
    }

    #[test]
    fn format_time_trims_zeros() {
        assert_eq!(format_time(nanoseconds(0.0)), "0 ns");
        assert_eq!(format_time(nanoseconds(2e6)), "2 ms");
        assert_eq!(format_time(nanoseconds(1.5e6)), "1.5 ms");
    }

    #[cfg(feature = "egui")]
    mod hover {
        use super::super::{plot_hover_label, waveform_hover_label};
        use crate::types::nanoseconds;
        use crate::utils::format_time;
        use egui_plot::{HoverPosition, PlotPoint};

        #[test]
        fn waveform_hover_shows_raw_and_formatted_time() {
            let point = HoverPosition::NearDataPoint {
                plot_name: "ch #1",
                position: PlotPoint { x: 4000.0, y: 12.5 },
                index: 500,
            };
            assert_eq!(
                waveform_hover_label(&point).unwrap(),
                format!(
                    "ch #1\nx = 500  ({})\ny = 12.500",
                    format_time(nanoseconds(4000.0))
                )
            );

            // вне точки данных — те же координаты, без имени серии
            let elsewhere = HoverPosition::Elsewhere {
                position: PlotPoint { x: 0.0, y: -3.0 },
            };
            assert_eq!(
                plot_hover_label(&elsewhere, |_| None, |_| Some("keV".to_owned())).unwrap(),
                "x = 0\ny = -3  (keV)"
            );
        }
    }

    #[test]
    fn bin_time_is_tick_per_bin() {
        assert_eq!(time_ns(bin_time(0)), 0);
        assert_eq!(time_ns(bin_time(5)), 5 * TICK_NS as u64);
    }

    #[test]
    fn repair_frame_times_anchors_broken_segment_after_valid_timeline() {
        let times = [0, 100, 200, 1_000_000, 1_000_150];
        let repaired = repair_frame_times(&times, 1_000);

        // валидные кадры идут как есть
        assert_eq!(repaired.times[..3], [Some(0), Some(100), Some(200)]);
        // сломанный сегмент — сразу за валидной шкалой, интервалы сохранены
        assert_eq!(repaired.times[3], Some(200));
        assert_eq!(repaired.times[4], Some(350));
        // оба таймстампа поправлены на одну и ту же поправку ~1 ms
        assert_eq!(repaired.repaired, 2);
        assert_eq!(repaired.shifts_ns, [1_000_000 - 200]);
    }

    #[test]
    fn repair_frame_times_each_segment_gets_own_anchor() {
        // два разных сегмента сбоя: каждый ставится за концом предыдущего
        let times = [0, 1_000_000, 5_000_000, 5_000_500, 9_000_000, 9_000_500];
        let repaired = repair_frame_times(&times, 2_000_000);

        assert_eq!(repaired.times[0], Some(0));
        assert_eq!(repaired.times[1], Some(1_000_000));
        assert_eq!(repaired.times[2], Some(1_000_000));
        assert_eq!(repaired.times[3], Some(1_000_500));
        assert_eq!(repaired.times[4], Some(1_000_500));
        assert_eq!(repaired.times[5], Some(1_001_000));
        assert_eq!(repaired.repaired, 4);
        // поправка своя у каждого сегмента: он заякорен за своей шкалой
        assert_eq!(repaired.shifts_ns, [4_000_000, 9_000_000 - 1_000_500]);
    }

    #[test]
    fn repair_frame_times_drops_garbage_beyond_segment_span() {
        // интервалы 500 ≤ bound → один сегмент, но его длина 1200 > bound:
        // хвост — не сдвиг константой, а мусор
        let repaired = repair_frame_times(&[0, 1_000_000, 1_000_500, 1_001_200], 1_000);

        assert_eq!(repaired.times[0], Some(0));
        assert_eq!(repaired.times[1], Some(0));
        assert_eq!(repaired.times[2], Some(500));
        assert_eq!(repaired.times[3], None);
        assert_eq!(repaired.repaired, 2);
        assert_eq!(repaired.shifts_ns, [1_000_000]);
    }
}
