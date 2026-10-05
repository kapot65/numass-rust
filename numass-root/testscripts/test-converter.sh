#!/usr/bin/env bash
# Сборка и конвертация тестовой точки.
# Использование: ./testscripts/test-converter.sh <path-to-point> [output.root]
set -euo pipefail

cd "$(dirname "$0")/.."

POINT="${1:-}"
if [[ -z "$POINT" ]]; then
  echo "Использование: $0 <path-to-point> [output.root]" >&2
  exit 1
fi

OUTPUT="${2:-build/point.root}"

# VDT_PREFIX — путь к локально собранной Vdt (если она не в системных путях).
CMAKE_ARGS=()
if [[ -n "${VDT_PREFIX:-}" ]]; then
  CMAKE_ARGS+=("-DCMAKE_PREFIX_PATH=$VDT_PREFIX")
fi

cmake -S . -B build "${CMAKE_ARGS[@]}"
cmake --build build -j

./build/convert-to-root "$POINT" -o "$OUTPUT"
echo "Готово: $OUTPUT"
