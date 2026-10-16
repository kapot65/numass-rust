"""Обработка точки алгоритмом Max после предобработки из preprocess.py.

Цепь: load_point → load_meta → extract_waveforms → apply_waveforms → extract_processed → to_polars().
`meta` — полный JSON метаданных, словарь. `extract_processed` читает его как NumassMeta.
`ProcessParams` по умолчанию переводит амплитуду в кэВ (`convert_to_kev=True`).
После to_polars() это polars.DataFrame, шаги модуля на неё не вызываются.
"""

from pathlib import Path

import numass_processing as n

POINT = Path(__file__).resolve().parents[1] / "resources" / "test" / "p4(30s)(HV1=18300)"


def main() -> None:
    point = n.load_point(POINT)
    meta = n.load_meta(POINT)
    waveforms = n.extract_waveforms(point)
    processed = n.apply_waveforms(waveforms, [])
    events = n.extract_processed(
        processed, waveforms, n.ProcessParams(n.Algorithm.Max()), meta
    )
    table = events.to_polars()
    print(meta)
    print(table)


if __name__ == "__main__":
    main()
