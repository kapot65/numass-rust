import dfparser

import py_numass

_, _, data = dfparser.parse_from_file("./resources/test/p4(30s)(HV1=18300)")

(times, amps) = py_numass.point_to_amps_max(data)
(_, amps_raw) = py_numass.point_to_amps_max(data, convert_to_kev=False)
(_, amps_likhovid) = py_numass.point_to_amps_likhovid(data, left=3, right=18)
(_, amps_firstpeak) = py_numass.point_to_amps_firstpeak(data)

