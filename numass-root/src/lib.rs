use std::{
    ffi::{c_char, CStr, CString},
    path::PathBuf,
};

use numass::protos::rsb_event;
use processing::{
    process::{extract_events, Algorithm, ProcessParams, TRAPEZOID_DEFAULT},
    types::FrameEvent,
};

use cxx::CxxString;
use protobuf::Message;

/// cbindgen:prefix-with-name
#[repr(C)]
#[derive(Debug)]
pub enum H {
    Foo(i16),
    Bar { x: u8, y: i16 },
    Baz,
}

#[no_mangle]
pub extern "C" fn say_hello() {
    println!("Hello from Rust!");
}

#[no_mangle]
pub extern "C" fn eat_string(s: *const c_char) {
    let string = unsafe { CStr::from_ptr(s) }.to_str().map(String::from);
    println!("Got string: {:?}", string);
}

#[no_mangle]
pub extern "C" fn eat_callback(callback: extern "C" fn(*const c_char)) {
    let message = CString::new("Hello from Rust!").expect("CString::new failed");
    callback(message.as_ptr());
}

#[no_mangle]
pub extern "C" fn eat_enum(h: H) {
    println!("h = {h:?}");
}

#[no_mangle]
/// Необходимо использовать std::move !!!
/// Или не удалять объект на время выполнения функции
pub extern "C" fn eat_cxx_string(str: &CxxString) {
    println!("Got CxxString: {:?}", str.to_str().unwrap().to_owned());
}

#[no_mangle]
pub extern "C" fn eat_algorithm(algo: Algorithm) {
    println!("Got algorithm: {algo:?}");
}

#[no_mangle]
/// возвращает [TRAPEZOID_DEFAULT] в С++ виде
pub extern "C" fn get_trapeziod_default() -> Algorithm {
    TRAPEZOID_DEFAULT.clone()
}

#[no_mangle]
pub extern "C" fn process_point(
    path: &CxxString,
    algo: &Algorithm,
    fill_fn: extern "C" fn(u64, u8, f32),
) {
    let filepath = PathBuf::from(path.to_str().unwrap());

    let (meta, point) = {
        let mut point_file = std::fs::File::open(filepath).unwrap();
        let message =
            dataforge::read_df_message_sync::<numass::NumassMeta>(&mut point_file).unwrap();
        let point =
            rsb_event::Point::parse_from_bytes(&message.data.unwrap_or_default()[..]).unwrap();
        (message.meta, point)
    };

    let (frames, _) = extract_events(
        Some(meta),
        point,
        &ProcessParams {
            algorithm: algo.clone(),
            convert_to_kev: true,
        },
    );

    frames.into_iter().for_each(|(frame_time, events)| {
        events.into_iter().for_each(|(offset, event)| {
            if let FrameEvent::Event {
                channel, amplitude, ..
            } = event
            {
                fill_fn(frame_time + offset as u64, channel, amplitude);
            }
        })
    });
}

// #[no_mangle]
// pub extern "C" fn eat_cxx_string_ref(str: &CxxString) {
//     println!("Got CxxString ref: {:?}", str.len());
// }

// uniffi::include_scaffolding!("processing");

// use numass::protos::rsb_event;
// use processing::{process::{ProcessParams, TRAPEZOID_DEFAULT}, types::FrameEvent};
// use protobuf::Message;

// pub fn add(left: u32, right: u32) -> u32 {
//     left + right
// }

// pub fn vec_from_rust(val: u32) -> Vec<u32> {
//     vec![val, val, val, val]
// }

// pub struct NumassEvent {
//     pub offset: u16,
//     pub event: FrameEvent
// }

// pub struct NumassTrigger {
//     pub time: u64,
//     pub events: Vec<NumassEvent>
// }

// pub fn process_point(filepath: String) -> Vec<NumassTrigger> {

//     let mut point_file = std::fs::File::open(filepath).unwrap();
//     let message = dataforge::read_df_message_sync::<numass::NumassMeta>(&mut point_file).unwrap();

//     let point = rsb_event::Point::parse_from_bytes(&message.data.unwrap_or_default()[..]).unwrap();

//     let frames = processing::process::extract_events(point, &ProcessParams {
//         algorithm: TRAPEZOID_DEFAULT,
//         convert_to_kev: true
//     });

//     frames.into_iter().map(|(time, events)| {
//         let events = events.into_iter().map(|(offset, event)| {
//             NumassEvent {
//                 offset, event
//             }
//         }).collect::<Vec<_>>();

//         NumassTrigger {
//             time, events
//         }
//     }).collect::<Vec<_>>()
// }
