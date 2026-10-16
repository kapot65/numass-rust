"""Постобработка событий после цепи из process.py.

Цепь: load_point → load_meta → from_point → extract_waveforms → apply_waveforms
→ extract_processed → post_process → to_polars().
`from_point` отдаёт Preprocess словарём. `post_process` читает этот словарь как Preprocess.
`merge_close_events` сливает соседние события кадра. `cut_bad_blocks` смотрит `bad_blocks`
из словаря; на этой точке HV выше порога, и список пуст.
После to_polars() это polars.DataFrame, шаги модуля на неё не вызываются.
"""

from pathlib import Path

import numass_processing as n

POINT = Path(__file__).resolve().parents[1] / "resources" / "test" / "p4(30s)(HV1=18300)"


def main() -> None:
    point = n.load_point(POINT)
    meta = n.load_meta(POINT)
    preprocess = n.from_point(point, meta)
    waveforms = n.extract_waveforms(point)
    processed = n.apply_waveforms(waveforms, [])
    events = n.extract_processed(
        processed, waveforms, n.ProcessParams(n.Algorithm.Max()), meta
    )
    events = n.post_process(
        events,
        preprocess,
        n.PostProcessParams(cut_bad_blocks=True, merge_close_events=True),
    )
    table = events.to_polars()
    print(preprocess)
    print(table)


if __name__ == "__main__":
    main()
