use std::vec;

use crate::agent::EventBus;
use common::FrameChannel;
use yew::{Component, Context, Html};
use yew_agent::{Bridge, Bridged};
use plotters::prelude::*;
use plotters::drawing::IntoDrawingArea;

use yew::prelude::*;


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


        html!{
            <div>{"AAAA"}</div>
        }

    }
}
