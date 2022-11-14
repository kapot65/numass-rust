use std::collections::HashSet;
use common::FrameChannel;
use yew_agent::{Agent, AgentLink, Context, HandlerId};

use tracing::*;

pub struct EventBus {
    link: AgentLink<EventBus>,
    subscribers: HashSet<HandlerId>,
}

impl Agent for EventBus {
    type Reach = Context<Self>;
    type Message = ();
    type Input = common::WS;
    type Output = Vec<FrameChannel>;

    fn create(link: AgentLink<Self>) -> Self {

        tracing::info!("creating dispatcher");

        Self {
            link,
            subscribers: HashSet::new(),
        }
    }

    fn update(&mut self, _msg: Self::Message) {}

    fn handle_input(&mut self, msg: Self::Input, _id: HandlerId) {
        
        match msg {
            common::WS::GraphFrame(s) => {
                // tracing::info!("{:?}", &s);
                for sub in self.subscribers.iter()
                    .filter(|s| s.is_respondable()) {

                        let a = s.iter().map(|ch| {
                            FrameChannel {
                                id: ch.id,
                                bins: ch.bins.clone()
                            }
                        }).collect();
                        
                        self.link.respond(*sub, a);     
                }
            }
        }
    }

    fn connected(&mut self, id: HandlerId) {
        info!("{:?}",id);
        self.subscribers.insert(id);
    }

    fn disconnected(&mut self, id: HandlerId) {
        self.subscribers.remove(&id);
    }
}
