"""Стандартный препроцессинг точки: шаги предобработки (нету), затем таблица Polars.

Цепь: load_point → extract_waveforms → apply_waveforms → to_polars().
После to_polars() это polars.DataFrame, шаги модуля на неё не вызываются.
"""

from pathlib import Path

import numass_processing as n

POINT = Path(__file__).resolve().parents[1] / "resources" / "test" / "p4(30s)(HV1=18300)"


def main() -> None:
    point = n.load_point(POINT)
    waveforms = n.extract_waveforms(point)
    processed = n.apply_waveforms(waveforms, [])
    table = processed.to_polars()
    print(table)


if __name__ == "__main__":
    main()
