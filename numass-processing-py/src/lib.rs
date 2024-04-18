use numpy::{ndarray::Dim, PyArray, PyArray2, PyArray1};
use pyo3::prelude::*;
use protobuf::Message;
use processing::{numass::protos::rsb_event, process::{extract_events, Algorithm, ProcessParams}};

type PyProcessedPoint<'py> = (&'py PyArray<u64, Dim<[usize; 1]>>, &'py PyArray<f32, Dim<[usize; 2]>>);

#[pymodule]
fn py_numass(_py: Python, m: &PyModule) -> PyResult<()> {

    fn point_to_amps<'py>(
        py: Python<'py>, point_data: &[u8], 
        algorithm: Algorithm, convert_to_kev: bool
    ) -> Result<PyProcessedPoint<'py>, PyErr> {
        let point = rsb_event::Point::parse_from_bytes(point_data).unwrap();

        let amplitudes = extract_events(
            &point, &ProcessParams { algorithm: algorithm.to_owned(), convert_to_kev }
        );

        let times = PyArray1::<u64>::zeros(py, amplitudes.len(), false);
        let amps = PyArray2::<f32>::zeros(py, [amplitudes.len(), 7], false);

        amplitudes.iter().enumerate().for_each(|(idx, (time, channels))| {
            unsafe {
                times.uget_raw(idx).write(*time);
            }
            for (ch_id, events) in channels {
                for (_, amp) in events {
                    if !(0..7).contains(ch_id) {
                        panic!("channel id {ch_id} not in range (0..7)")
                    }
                    unsafe {
                        amps.uget_raw([idx, *ch_id]).write(*amp);
                    }
                }
            }
        });
        Ok((times, amps))
    }

    #[pyfn(m)]
    fn point_to_amps_max<'py>(
        py: Python<'py>, 
        point_data: &[u8], 
        convert_to_kev: Option<bool>
    ) -> Result<PyProcessedPoint<'py>, PyErr> {

        let algorithm = Algorithm::Max;
        let convert_to_kev = convert_to_kev.unwrap_or(true);
        point_to_amps(py, point_data, algorithm, convert_to_kev)
    }

    #[pyfn(m)]
    fn point_to_amps_likhovid<'py>(
        py: Python<'py>, point_data: &[u8], 
        left: Option<usize>, right: Option<usize>, 
        convert_to_kev: Option<bool>
    ) -> Result<PyProcessedPoint<'py>, PyErr> {
        
        let algorithm = {
            let left = left.unwrap_or(15);
            let right = right.unwrap_or(35);
            Algorithm::Likhovid { left, right }
        };
        let convert_to_kev = convert_to_kev.unwrap_or(true);
        point_to_amps(py, point_data, algorithm, convert_to_kev)
    }

    #[pyfn(m)]
    fn point_to_amps_firstpeak<'py>(
        py: Python<'py>, point_data: &[u8], 
        threshold: Option<i16>, left: Option<usize>,
        convert_to_kev: Option<bool>
    ) -> Result<PyProcessedPoint<'py>, PyErr> {
        let algorithm = {
            let left = left.unwrap_or(8);
            let threshold = threshold.unwrap_or(10);
            Algorithm::FirstPeak { threshold,  left }
        };
        let convert_to_kev = convert_to_kev.unwrap_or(true);
        point_to_amps(py, point_data, algorithm, convert_to_kev)
    }

    Ok(())
}