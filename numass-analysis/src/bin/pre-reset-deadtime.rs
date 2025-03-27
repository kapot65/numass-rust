//! # pre-reset-deadtime
//! Скрипт рисует гистограмму времен между ближайшими событиями в файле данных.
//! Используется для оценки аппаратного мертвого времени перед  пилы.
//!
use std::path::PathBuf;

use plotly::{common::Title, layout::Axis, Layout, Plot};

use processing::{
    histogram::PointHistogram,
    process::{ProcessParams, SkipOption, TRAPEZOID_DEFAULT},
    storage::process_point,
    types::FrameEvent,
};

#[tokio::main]
async fn main() {
    // let point_path = "/data-fast/numass-server/2024_11/Tritium_2_1/set_4/p196(20s)(HV1=10000)";
    let point_path = "/data-fast/numass-server/2024_11/Tritium_2_1/set_4/p0(30s)(HV1=14000)";

    let mut process = ProcessParams {
        algorithm: TRAPEZOID_DEFAULT,
        convert_to_kev: true,
    };

    match &mut process.algorithm {
        processing::process::Algorithm::Trapezoid { skip, .. } => *skip = SkipOption::Good,
        _ => unreachable!(),
    }

    let (_, events) = process_point(&PathBuf::from(point_path), &process, None)
        .await
        .expect("Failed to process point");

    let (events, _) = events.expect("Failed to get events");

    let mut hist = PointHistogram::new_step((-100.0 * 8.0)..(252.0 * 8.0), 8.0);

    events.into_iter().for_each(|(_, frame)| {
        let mut reset_pos = None;

        for (offset, event) in &frame {
            if let FrameEvent::Reset { .. } = event {
                reset_pos = Some(*offset);
                break;
            }
        }

        if let Some(reset_pos) = reset_pos {
            for (offset, event) in frame {
                if let FrameEvent::Event { .. } = event {
                    let delta = reset_pos as f32 - offset as f32;
                    hist.add(0, delta);
                }
            }
        }
    });

    {
        println!("delta\tcounts");
        for (idx, x) in hist.x.iter().enumerate() {
            println!("{x}\t{}", hist.channels[&0][idx]);
        }

        let mut plot = Plot::new();

        let layout = Layout::new()
            .title(Title::new(&format!(
                "delta(ev_offset, reset_offset) for {point_path}"
            )))
            .x_axis(Axis::new().title(Title::new("time delta, ns")))
            .y_axis(Axis::new().type_(plotly::layout::AxisType::Log))
            .height(1000);

        plot.set_layout(layout);
        hist.draw_plotly(&mut plot, None);
        plot.show();
    }
}
