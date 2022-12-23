
use dataforge::protos::rsb_event;

pub fn frame_to_waveform(frame: &rsb_event::point::channel::block::Frame) -> Vec<i16>{
    let waveform_len = frame.data.len() / 2;
    (0..waveform_len).map(|idx| {
        i16::from_le_bytes(frame.data[idx*2..idx*2+2].try_into().unwrap())
    }).collect::<Vec<_>>()
}

/// Parabolic event amplitude correction correction
pub fn correct_amp(y0: f32, y1: f32, y2: f32) -> (f32, f32) {
    (
        // calculated with SymPy
        (y0 - y2)/(2.0*(y0 - 2.0*y1 + y2)),
        (-(y0*y0)/8.0 + y0*y1 + y0*y2/4.0 - 2.0 * y1 * y1 + y1*y2 - (y2*y2)/8.0)/(y0 - 2.0 * y1 + y2)
    )
}

pub fn find_first_peak(waveform: &Vec<i16>, threshold: f32, baseline: f32) -> usize {
    waveform.iter().enumerate().find(|(idx, amp)| {
        let amp = **amp as f32 - baseline;
        amp > threshold &&
        (*idx == 0 || waveform[idx - 1] as f32 - baseline <= amp)&&
        (*idx == waveform.len() - 1 || waveform[idx + 1] as f32 - baseline <= amp)
    }).map(|(idx, _)| idx).unwrap()
}