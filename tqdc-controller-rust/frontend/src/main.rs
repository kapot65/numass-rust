// use frontend::protos; 

// use wasm_bindgen_futures::spawn_local;

use yew::prelude::*;
use yew_agent::Dispatched;
// use yew_router::prelude::*;
// use wasm_sockets::*;

use frontend::agent::{EventBus};
use frontend::simple_sub::Subscriber;


use tracing::info;


use gloo_net::websocket::{futures::WebSocket};
use wasm_bindgen_futures::spawn_local;
use futures::{StreamExt, SinkExt};


#[cfg(target_arch = "wasm32")]
fn main() {

    console_error_panic_hook::set_once();
    tracing_wasm::set_as_global_default();

    tracing::info!("AAAA");

    let ws = WebSocket::open("ws://localhost:8080/api/").unwrap();
    let (mut tx, mut rx) = ws.split();

    spawn_local(async move {
        tx.send(gloo_net::websocket::Message::Text(String::from("test"))).await.unwrap();
        // write.send(gloo_net::websocket::Message::Text(String::from("test 2"))).await.unwrap();
    });

    spawn_local(async move {
        
        let mut dispatcher = EventBus::dispatcher();
        
        while let Some(msg) = rx.next().await {
            tracing::info!("{msg:?}")
            // match msg.as_ref().unwrap() {
            //     gloo_net::websocket::Message::Text(_) => info!("text!!!"),
            //     gloo_net::websocket::Message::Bytes(bytes) => {
            //         dispatcher.send(bendy::serde::from_bytes::<common::WS>(&bytes[..]).unwrap());  
            //     }
            // }
        }
        info!("WebSocket Closed")
    });

    yew::start_app::<Subscriber>();
}
