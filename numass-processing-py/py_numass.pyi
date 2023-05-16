from typing import Literal, Tuple
import numpy

def point_to_amps(
    point_data: bytes, 
    algorithm: Literal['max', 'likhovid'] = 'likhovid', 
    likhovid_range: Tuple[int, int] = (6, 36), 
    convert_to_kev: bool = True) -> Tuple[
        numpy.ndarray[int, numpy.dtype[numpy.uint64]], 
        numpy.ndarray[Tuple[int, int], numpy.dtype[numpy.uint64]]]:
    """Converts waveforms to amplitudes.

    Args:
        point_data: Point binary data (in protobuf format).
        likhovid_range: Integration range for Likhovid algorithm.
        convert_to_kev: Transform amplitude to keV.

    Returns:
        Times in nanoseconds, amplitudes (each row correspon).
    """
    ...
    

def point_to_amps_max(
    point_data: bytes, 
    convert_to_kev: bool = True) -> Tuple[
        numpy.ndarray[int, numpy.dtype[numpy.uint64]], 
        numpy.ndarray[Tuple[int, int], numpy.dtype[numpy.uint64]]]:
    """Converts waveforms to amplitudes using Max algorithm.

    Args:
        point_data: Point binary data (in protobuf format).
        convert_to_kev: Transform amplitude to keV.

    Returns:
        Times in nanoseconds, amplitudes (sparce matrix).
    """
    ...


def point_to_amps_likhovid(
    point_data: bytes,
    left: int = 15,
    right: int = 35,
    convert_to_kev: bool = True) -> Tuple[
        numpy.ndarray[int, numpy.dtype[numpy.uint64]], 
        numpy.ndarray[Tuple[int, int], numpy.dtype[numpy.uint64]]]:
    """Converts waveforms to amplitudes using Likhovid algorithm.

    Args:
        point_data: Point binary data (in protobuf format).
        left: left range,
        right: right range,
        convert_to_kev: Transform amplitude to keV.

    Returns:
        Times in nanoseconds, amplitudes (sparce matrix).
    """
    ...


def point_to_amps_firstpeak(
    point_data: bytes,
    treshold: int = 10,
    left: int = 8,
    convert_to_kev: bool = True) -> Tuple[
        numpy.ndarray[int, numpy.dtype[numpy.uint64]], 
        numpy.ndarray[Tuple[int, int], numpy.dtype[numpy.uint64]]]:
    """Converts waveforms to amplitudes using Likhovid algorithm.

    Args:
        point_data: Point binary data (in protobuf format).
        treshold: treshold in ADC,
        left: left range,
        convert_to_kev: Transform amplitude to keV.

    Returns:
        Times in nanoseconds, amplitudes (sparce matrix).
    """
    ...
    