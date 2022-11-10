use std::vec;

use crate::agent::EventBus;
use common::FrameChannel;
use yew::{Component, Context, Html};
use yew_agent::{Bridge, Bridged};
use plotters::prelude::*;
use plotters::drawing::IntoDrawingArea;


pub enum Msg {
    NewMessage(Vec<FrameChannel>),
}

pub struct Subscriber {
    message: Vec<FrameChannel>,
    _producer: Box<dyn Bridge<EventBus>>,
}

impl Component for Subscriber {
    type Message = Msg;
    type Properties = ();

    fn create(ctx: &Context<Self>) -> Self {
        Self {
            message: vec![FrameChannel {
                id: 0,
                bins: vec![100,1000]
            }],
            _producer: EventBus::bridge(ctx.link().callback(Msg::NewMessage)),
        }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        match msg {
            Msg::NewMessage(s) => {
                // tracing::info!("{:?}", s);
                self.message = s;
                true
            }
        }
    }

    fn view(&self, _ctx: &Context<Self>) -> Html {

        // self.message.iter().enumerate().map(|(i, y)| {
        //     (i as f64, y as f64)
        // });

        let mut buff = String::new();
        {
            let root_drawing_area = SVGBackend::with_string(&mut buff, (512,512))
            .into_drawing_area();

            root_drawing_area.fill(&WHITE).unwrap();

            let mut chart = ChartBuilder::on(&root_drawing_area)
                .x_label_area_size(20)
                .y_label_area_size(60)
                .build_cartesian_2d(-1.0..100.0, -1.0..1000.0)
                .unwrap();

            chart.configure_mesh()
                // We can customize the maximum number of labels allowed for each axis
                .x_labels(5)
                .y_labels(5)
                // We can also change the format of the label text
                .y_label_formatter(&|x| format!("{:.3}", x))
                .draw().unwrap();
        
            for channel in &self.message {
                chart.draw_series(LineSeries::new(
                    channel.bins[..].iter().enumerate().map(|(i, y)| {
                        (i as f64, *y as f64)
                    }),
                    // (-314..314).map(|x| x as f64 / 100.0).map(|x| (x, x.sin())),
                    &BLACK
                )).unwrap();
            }

            
        }
        // tracing::info!("{}", buff);


        let div = gloo_utils::document().create_element("div").unwrap();
        div.set_inner_html(&buff[..]);

        Html::VRef(div.into())
    }
}
