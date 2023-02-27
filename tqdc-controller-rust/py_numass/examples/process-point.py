import dfparser

import py_numass

_, _, data = dfparser.parse_from_file("../test-data/points/p4(30s)(HV1=18300)")

(times, amps) = py_numass.point_to_amps(data)
(_, amps_raw) = py_numass.point_to_amps(data, algorithm="max", convert_to_kev=False)
(_, amps_likhovid_custom) = py_numass.point_to_amps(data, algorithm="max", likhovid_range=[3, 18])


