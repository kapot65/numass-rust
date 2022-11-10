use serde::{Serialize, Deserialize};


#[derive(Serialize, Deserialize, Debug)]
pub struct FrameChannel {
    pub id: u16,
    pub bins: Vec<i16>
}

#[derive(Serialize, Deserialize, Debug)]
pub enum WS {
    #[serde(rename="GF")]
    GraphFrame(Vec<FrameChannel>),
}