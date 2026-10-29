#!python
"""Пишет numass-точку в ROOT.

JSON флагов читает Rust. Таблица идёт в файл через Polars, Arrow, Awkward и uproot.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path
from typing import NoReturn

__version__ = "3.2.1"


def main() -> None:
    parser = argparse.ArgumentParser(prog="convert-to-root", description=__doc__)
    parser.add_argument(
        "--version",
        action="version",
        version=f"%(prog)s {__version__}",
        help="показать версию и выйти",
    )
    parser.add_argument("path", help="путь к точке")
    parser.add_argument(
        "--preprocess",
        metavar="json",
        help="JSON PreprocessParams. Без --process файл заканчивается на этом шаге",
    )
    parser.add_argument(
        "-p",
        "--process",
        metavar="json",
        help="JSON ProcessParams. Только вместе с --preprocess",
    )
    parser.add_argument(
        "--postprocess",
        metavar="json",
        help="JSON PostProcessParams. Только вместе с --process",
    )
    parser.add_argument("-o", "--output", help="файл .root, по умолчанию <имя точки>.root")
    args = parser.parse_args()

    if args.process is not None and args.preprocess is None:
        _die("обработка задаётся только вместе с --preprocess")
    if args.postprocess is not None and args.process is None:
        _die("постобработка задаётся только вместе с --process")

    import numass_processing as n

    preprocess = _params(n.PreprocessParams, "preprocess", args.preprocess)
    process = _params(n.ProcessParams, "process", args.process)
    postprocess = _params(n.PostProcessParams, "postprocess", args.postprocess)

    point_path = Path(args.path)
    output = Path(args.output) if args.output else Path(point_path.name + ".root")

    try:
        point = n.load_point(point_path)
        table = _table(n, point, point_path, preprocess, process, postprocess)
    except (OSError, ValueError, TypeError, KeyError) as exc:
        _die(str(exc))

    _write_root(table, output)


def _table(n, point, point_path, preprocess, process, postprocess):
    waveforms = n.extract_waveforms(point)
    if preprocess is None:
        return waveforms.to_polars()

    processed = n.apply_waveforms(waveforms, preprocess.steps)
    if process is None:
        return processed.to_polars()

    events = n.extract_processed(processed, waveforms, process)
    if postprocess is None:
        return events.to_polars()

    events = n.post_process(events, n.from_point(point, n.load_meta(point_path)), postprocess)
    return events.to_polars()


def _params(cls, kind: str, text: str | None):
    if text is None:
        return None
    try:
        return cls.from_json(text)
    except ValueError as exc:
        _die(f"Failed to parse {kind} parameters: {exc}")


def _die(message: str) -> NoReturn:
    print(message, file=sys.stderr)
    raise SystemExit(1)


def _write_root(frame, output: Path) -> None:
    try:
        import awkward as ak
        import uproot
    except ImportError:
        _die("нужен extra root: uv tool install '.[root]'")

    try:
        array = ak.from_arrow(frame.to_arrow())
        with uproot.recreate(output) as file:
            file["T"] = array
    except (OSError, ValueError, TypeError, KeyError) as exc:
        _die(str(exc))


if __name__ == "__main__":
    main()
