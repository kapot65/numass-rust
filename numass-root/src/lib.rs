use std::path::PathBuf;

use numass::{protos::rsb_event, Reply};
use processing::{
    postprocess::{post_process, PostProcessParams}, process::{extract_events, ProcessParams}, types::FrameEvent
};

use cxx::CxxString;
use protobuf::Message;

#[repr(C)]
pub struct ProcessedTree {
    pub time: u64,
    pub channel: u8,
    pub amplitude: f32,
    pub size: u16,
    pub tree_ref: usize
}

#[no_mangle]
/// возвращает [ProcessParams::default] в С++ виде
pub extern "C" fn get_process_default() -> ProcessParams {
    ProcessParams::default()
}


#[no_mangle]
pub extern "C" fn parse_process(serialized: &CxxString, ok: Option<&mut bool>) -> ProcessParams {
    let mut params = ProcessParams::default();
    if let Ok(serialized_str) = serialized.to_str() {
        if let Ok(parsed) = serde_json::from_str::<ProcessParams>(serialized_str) {
            if let Some(ok) = ok {
                *ok = true;
            }
            params = parsed
        } else {
            if let Some(ok) = ok {
                *ok = false;
            }
        }
    } else {
        if let Some(ok) = ok {
            *ok = false;
        }
    }
    params
}

#[no_mangle]
pub extern "C" fn parse_postprocess(serialized: &CxxString, ok: Option<&mut bool>) -> PostProcessParams {
    let mut params = PostProcessParams::default();
    if let Ok(serialized_str) = serialized.to_str() {
        if let Ok(parsed) = serde_json::from_str::<PostProcessParams>(serialized_str) {
            if let Some(ok) = ok {
                *ok = true;
            }
            params = parsed
        } else {
            if let Some(ok) = ok {
                *ok = false;
            }
        }
    } else {
        if let Some(ok) = ok {
            *ok = false;
        }
    }
    params
}

// TODO: finish this function
#[no_mangle]
pub extern "C" fn is_point(
    path: &CxxString,
) -> bool {
    let filepath = PathBuf::from(path.to_str().unwrap());
    if let Ok((_, Reply::AcquirePoint {
        ..
    })) = dataforge::read_df_header_and_meta_sync(&mut std::fs::File::open(filepath).unwrap()) {
        true
    } else {
        false
    }
}

#[no_mangle]
pub extern "C" fn process_point(
    path: &CxxString,
    processed_tree: &mut ProcessedTree,
    fill_fn: extern "C" fn(&mut ProcessedTree, u64, u8, f32, u16),
    process: &ProcessParams,
    postprocess: Option<&PostProcessParams>
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

    let frames = {
        let (frames, preprocess) = extract_events(Some(meta), point, process);
        if let Some(postprocess) = postprocess {
            let (frames, _) = post_process((frames, preprocess), &postprocess);
            frames
        } else {
            frames
        }
    };

    frames.into_iter().for_each(|(frame_time, events)| {
        events.into_iter().for_each(|(offset, event)| {
            if let FrameEvent::Event {
                channel, amplitude, size
            } = event
            {
                fill_fn(processed_tree, frame_time + offset as u64, channel, amplitude, size);
            }
        })
    });
}
