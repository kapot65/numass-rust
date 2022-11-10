// use frontend::protos; 

// use wasm_bindgen_futures::spawn_local;

use yew::prelude::*;
use yew_agent::Dispatched;
// use yew_router::prelude::*;
// use wasm_sockets::*;

use frontend::agent::{EventBus};
use frontend::graph_view::Subscriber;


use tracing::info;


use gloo_net::websocket::{futures::WebSocket};
use wasm_bindgen_futures::spawn_local;
use futures::StreamExt;


#[function_component(SimpleUpdateCounter)]
fn simpleUpdateCounter() -> Html {

    let counter = use_state(|| 0);
    // let interval: UseStateHandle<Option<Interval>> = use_state(|| None);

    // use_bridge<(|_| {

    // });

    {
        // let counter = counter.clone();
        // let interval = interval.clone();
        use_effect_with_deps(move |_| {
            

            info!("use_effect_with_deps (no deps)");

            // interval.set(Some(Interval::new(1000, || {
            //     tracing::info!("AAAA")
            // })));

            // Make a call to DOM API after component is rendered
            // gloo_utils::document().set_title(&format!("You clicked {} times", *counter));

            // Perform the cleanup
            // || gloo_utils::document().set_title("You clicked 0 times")
            || ()
        }, ());
    }
    let onclick = {
        let counter = counter.clone();
        Callback::from(move |_| counter.set(*counter + 1))
    };

    html! {
        <div>
            <button {onclick}>{ format!("Increment to {}", *counter) }</button>
            <Subscriber></Subscriber>
        </div>
    }
}


#[cfg(target_arch = "wasm32")]
fn main() {

    console_error_panic_hook::set_once();
    tracing_wasm::set_as_global_default();

    let ws = WebSocket::open("ws://localhost:8082").unwrap();
    let (_, mut read) = ws.split();

    // spawn_local(async move {
    //     write.send(gloo_net::websocket::Message::Text(String::from("test"))).await.unwrap();
    //     write.send(gloo_net::websocket::Message::Text(String::from("test 2"))).await.unwrap();
    // });

    spawn_local(async move {
        
        let mut dispatcher = EventBus::dispatcher();
        

        while let Some(msg) = read.next().await {
            match msg.as_ref().unwrap() {
                gloo_net::websocket::Message::Text(_) => info!("text!!!"),
                gloo_net::websocket::Message::Bytes(bytes) => {
                    dispatcher.send(bendy::serde::from_bytes::<common::WS>(&bytes[..]).unwrap());  
                }
            }
        }
        info!("WebSocket Closed")
    });

    yew::start_app::<SimpleUpdateCounter>();
}
