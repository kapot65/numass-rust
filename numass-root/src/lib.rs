use std::{path::PathBuf, ptr::NonNull};

use dataforge::{
    read_df_header_and_meta_sync, read_df_header_and_rawmeta_sync, read_df_message_sync,
};
use numass::{protos::rsb_event, NumassMeta, Reply};
use processing::{
    postprocess::{post_process, PostProcessParams},
    process::{extract_events, ProcessParams},
    types::FrameEvent,
};

use cxx::CxxString;
use protobuf::Message;

#[repr(C)]
pub struct ProcessedTree {
    pub time: u64,
    pub channel: u8,
    pub amplitude: f32,
    pub size: u16,
    pub tree_ref: usize,
}

/// Проверяет, что строка является валидным JSON описанием [ProcessParams].
#[no_mangle]
pub extern "C" fn validate_process(serialized: &CxxString) -> bool {
    serialized
        .to_str()
        .ok()
        .map(|s| serde_json::from_str::<ProcessParams>(s).is_ok())
        .unwrap_or(false)
}

/// Проверяет, что строка является валидным JSON описанием [PostProcessParams].
#[no_mangle]
pub extern "C" fn validate_postprocess(serialized: &CxxString) -> bool {
    serialized
        .to_str()
        .ok()
        .map(|s| serde_json::from_str::<PostProcessParams>(s).is_ok())
        .unwrap_or(false)
}

/// Возвращает `true`, если файл является numass-точкой.
#[no_mangle]
pub extern "C" fn is_point(path: &CxxString) -> bool {
    let Ok(path_str) = path.to_str() else {
        return false;
    };
    let Ok(mut file) = std::fs::File::open(path_str) else {
        return false;
    };

    matches!(
        read_df_header_and_meta_sync(&mut file),
        Ok((_, Reply::AcquirePoint { .. }))
    )
}

/// Читает сырую мета-информацию точки в переданный буфер.
///
/// Возвращает длину прочитанных данных. Паникует, если буфер слишком мал.
#[no_mangle]
pub extern "C" fn read_meta_c(path: &CxxString, buffer: NonNull<u8>, len: usize) -> usize {
    let slice = unsafe { std::slice::from_raw_parts_mut(buffer.as_ptr(), len) };

    let filepath = PathBuf::from(path.to_str().unwrap());
    let (_, meta_str) =
        read_df_header_and_rawmeta_sync(&mut std::fs::File::open(filepath).unwrap()).unwrap();

    if len < meta_str.len() {
        panic!(
            "Buffer is too small ({len}) to hold the metadata ({})",
            meta_str.len()
        )
    }

    slice[0..meta_str.len()].copy_from_slice(&meta_str);

    meta_str.len()
}

fn process_params_from_json(serialized: &CxxString) -> ProcessParams {
    serialized
        .to_str()
        .ok()
        .and_then(|s| serde_json::from_str::<ProcessParams>(s).ok())
        .unwrap_or_default()
}

fn postprocess_params_from_json(serialized: &CxxString) -> PostProcessParams {
    serialized
        .to_str()
        .ok()
        .and_then(|s| serde_json::from_str::<PostProcessParams>(s).ok())
        .unwrap_or_default()
}

/// Обрабатывает точку и заполняет [ProcessedTree] через переданный `fill_fn`.
///
/// Параметры обработки передаются как JSON-строки (см. [ProcessParams] и
/// [PostProcessParams]). Пустая или невалидная строка означает параметры по
/// умолчанию; `None` для postprocess отключает пост-обработку.
#[no_mangle]
pub extern "C" fn process_point(
    path: &CxxString,
    processed_tree: &mut ProcessedTree,
    fill_fn: extern "C" fn(&mut ProcessedTree, u64, u8, f32, u16),
    process_json: &CxxString,
    postprocess_json: Option<&CxxString>,
) {
    let filepath = PathBuf::from(path.to_str().unwrap());

    let (meta, point) = {
        let mut point_file = std::fs::File::open(filepath).unwrap();
        let message = read_df_message_sync::<NumassMeta>(&mut point_file).unwrap();
        let point =
            rsb_event::Point::parse_from_bytes(&message.data.unwrap_or_default()[..]).unwrap();
        (message.meta, point)
    };

    let process = process_params_from_json(process_json);
    let postprocess = postprocess_json.map(postprocess_params_from_json);

    let frames = {
        let (frames, preprocess) = extract_events(Some(meta), point, &process);
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
                channel,
                amplitude,
                size,
            } = event
            {
                fill_fn(
                    processed_tree,
                    frame_time + offset as u64,
                    channel,
                    amplitude,
                    size,
                );
            }
        })
    });
}
