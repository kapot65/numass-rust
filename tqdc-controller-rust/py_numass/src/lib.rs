#[cfg(not(target_arch = "wasm32"))] use {
    numpy::{ndarray::Dim, PyArray, PyArray2, PyArray1},
    pyo3::{prelude::*, exceptions::PyValueError},
    protobuf::Message,
    numass::protos::rsb_event,
    processing::Algorithm
};

#[cfg(not(target_arch = "wasm32"))]
type PyProcessedPoint<'py> = (&'py PyArray<u64, Dim<[usize; 1]>>, &'py PyArray<f32, Dim<[usize; 2]>>);

#[cfg(not(target_arch = "wasm32"))]
#[pymodule]
fn py_numass(_py: Python, m: &PyModule) -> PyResult<()> {

    #[pyfn(m)]
    fn point_to_amps<'py>(
            py: Python<'py>, point_data: &[u8], algorithm: Option<&str>, likhovid_range: Option<[usize; 2]>,
            convert_to_kev: Option<bool>,) -> Result<PyProcessedPoint<'py>, PyErr> {
        let algorithm =  match algorithm {
            None => Algorithm::default(),
            Some("max") => Algorithm::Max,
            Some("likhovid") => {
                let [left, right] = likhovid_range.unwrap_or([6, 36]);
                Algorithm::Likhovid { left, right }
            },
            Some(_) => Err(PyValueError::new_err("algorithm must be 'max' or 'likhovid' "))?
        };
        let convert_to_kev = convert_to_kev.unwrap_or(true);

        let point = rsb_event::Point::parse_from_bytes(point_data).unwrap();

        let amplitudes = processing::extract_amplitudes(
            &point, &algorithm, convert_to_kev
        );

        let times = PyArray1::<u64>::zeros(py, amplitudes.len(), false);
        let amps = PyArray2::<f32>::zeros(py, [amplitudes.len(), 7], false);

        amplitudes.iter().enumerate().for_each(|(idx, (time, channels))| {
            unsafe {
                times.uget_raw(idx).write(*time);
            }
            for (ch_id, amp) in channels {
                if !(0..7).contains(ch_id) {
                    panic!("channel id {ch_id} not in range (0..7)")
                }
                unsafe {
                    amps.uget_raw([idx, *ch_id]).write(*amp);
                }
            }
        });
        Ok((times, amps))
    }

    Ok(())
}