from typing import Literal

def point_to_amps(
    point_data: bytes, 
    algorithm: Literal['max', 'likhovid'] = 'likhovid', 
    likhovid_range: (int, int) = (6, 36), 
    convert_to_kev: bool = True) -> (numpy.ndarray, numpy.ndarray):
    """Converts waveforms to amplitudes.

    Args:
        point_data: Point binary data (in protobuf format).
        algorithm: Convertion algorithm ('max' or 'likhovid').
        likhovid_range: Integration range for Likhovid algorithm.
        convert_to_kev: Transform amplitude to keV.

        
    Returns:
        (numpy.ndarray, numpy.ndarray): Times in nanoseconds, amplitudes.
    """
    ...