
use dataforge::protos::rsb_event;

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum Algorithm {
    Max,
    Likhovid {
        left: usize,
        right: usize
    }
}

 // TODO remove hardcode
const KEV_COEFF_MAX: [[f32; 2]; 7] = [
    [0.0597, 0.1481],
    [0.0608, 0.1942],
    [0.0634, 0.0839],
    [0.0627, 0.1587],
    [0.0626, 0.1675],
    [0.0675, 0.0930],
    [0.0580, 0.0923],
];

const KEV_COEFF_LIKHOVID: [[f32; 2]; 7] = [
    [0.134678, 0.09647 ],
    [0.141536, 0.060275],
    [0.147718, 0.027412],
    [0.150288, 0.038774],
    [0.15131 , 0.071923],
    [0.15336 , 0.029206],
    [0.136762, 0.041848]
];

pub fn convert_to_kev(amplitude: &f32, ch_id: u8, algorithm: &Algorithm) -> f32 {
    match algorithm {
        Algorithm::Max => {
            let [a, b] = KEV_COEFF_MAX[ch_id as usize];
            a * *amplitude as f32 + b
        },
        Algorithm::Likhovid { .. } => {
            let [a, b] = KEV_COEFF_LIKHOVID[ch_id as usize];
            a * *amplitude + b
        }
    }
}

pub fn waveform_to_event(waveform: &Vec<i16>, algorithm: &Algorithm) -> (u64, f32) {
    let baseline = waveform.iter().take(16).sum::<i16>() as f32 / 16.0;
    let (x, y)  = waveform.iter().enumerate().max_by_key(|(_, amp)| *amp).unwrap();

    match algorithm {
        
        Algorithm::Max => (x as u64 * 8,  *y as f32 + baseline),
        Algorithm::Likhovid { left, right } => {
            // TODO: move to processing
            let amplitude = {
                let left = if x >= *left {x - left} else { 0 };
                let right = std::cmp::min(waveform.len(), x + right);
                let crop =  &waveform[left..right];
                crop.iter().sum::<i16>() as f32 / crop.len() as f32
            };

            (x as u64 * 8, amplitude - baseline)
        }
    }
}

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