# numass-rust

Монорепозиторий Rust-проектов для обработки numass-данных.

## Подпроекты

| Каталог | Назначение |
| --- | --- |
| `dataforge-parser-rust` | Парсер протокола DataForge envelope |
| `dataforge-parser-numass` | Типы numass-данных для DataForge |
| `numass-processing` | Общие алгоритмы обработки numass-точек |
| `numass-analysis` | Скрипты обработки данных numass |
| `numass-processing-py` | Python-биндинги (`numass-processing`) |
| `numass-root` | C++-биндинги к CERN ROOT + утилита `convert-to-root` |
| `numass-server` | Сервер (кросс-компиляция) |
| `numass-viewers` | Визуализация (wasm) |
| `tqdc-controller-rust` | Контроллер TQDC (`frame-viewer`) |

Подробности — в `README.md` соответствующего подпроекта.

## Системные требования

### Общие (Rust-крейты)

- Rust (cargo)

### `numass-root` (дополнительно)

Для сборки `numass-root` — C++-биндингов к CERN ROOT — дополнительно нужны:

- CMake ≥ 3.20 (и генератор: Make или Ninja)
- Компилятор C++ (`g++`/`clang++`)
- [CERN ROOT](https://root.cern/) — `root-config` в `PATH`
- **Vdt** — зависимость ROOT:
  ```bash
  sudo apt-get install libvdt-dev
  ```
  либо соберите Vdt локально и передайте `-DCMAKE_PREFIX_PATH=/path/to/vdt`.

Полный список и инструкции — [`numass-root/README.md`](numass-root/README.md).
