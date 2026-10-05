# numass-root

Биндинги алгоритмов [`numass-processing`](../numass-processing) к CERN ROOT (C++) и
утилита `convert-to-root` для конвертации numass-точек в формат ROOT.

Rust-кран (`src/lib.rs`) через [cbindgen] генерирует C-совместимый заголовок
`bindings/processing-internal.h`. Поверх него лежит тонкая C++-обёртка
`bindings/processing.h`, которую использует `convert-to-root.cxx`.

[cbindgen]: https://github.com/mozilla/cbindgen

## Требования

- CMake ≥ 3.20 (и генератор: Make или Ninja)
- Rust (cargo)
- Компилятор C++ (`g++`/`clang++`)
- [CERN ROOT](https://root.cern/) (`root-config` в `PATH`)

## Сборка

Через Cargo (рекомендуется): `build.rs` сам вызывает CMake (тот через
[Corrosion] собирает Rust и линкует C++-утилиту) и кладёт бинарник
`convert-to-root` прямо в `target/release/`.

```bash
cargo build --release -p numass-root   # из корня воркспейса
# или: cd numass-root && cargo build --release
```

Бинарник появится в `target/release/convert-to-root` (без `--release` —
в `target/debug/`).

Как раньше, можно собрать и напрямую через CMake:

```bash
cmake -S . -B build            # по умолчанию Unix Makefiles (или -G Ninja)
cmake --build build -j
```

Сборка по умолчанию идёт в режиме `Release`. При первом запуске CMake через
[CPM] скачивает зависимости — [Corrosion] (интеграция Rust) и argparse.

> `find_package(ROOT)` требует **Vdt** — зависимость ROOT. Если получаете
> `Could NOT find Vdt`, установите её:
> ```bash
> sudo apt-get install libvdt-dev
> ```
> либо соберите Vdt локально и передайте `-DCMAKE_PREFIX_PATH=/path/to/vdt`.

[CPM]: https://github.com/cpm-cmake/CPM.cmake
[Corrosion]: https://github.com/corrosion-rs/corrosion

## Настройка clangd (Zed)

Чтобы clangd видел заголовки CERN ROOT (`TTree.h` и т.п.), ему нужен
`compile_commands.json`. Он генерируется CMake — в `CMakeLists.txt` уже включён
`CMAKE_EXPORT_COMPILE_COMMANDS`, а `.clangd` указывает на каталог `build/`.

```bash
# вручную:
cmake -S . -B build -DCMAKE_EXPORT_COMPILE_COMMANDS=ON

# или скриптом (Nushell):
nu setup-clangd.nu
```

После генерации перезапустите языковой сервер (в Zed: команда
`language server: restart`).

## Использование

```bash
./target/release/convert-to-root /path/to/point.df -o point.root
```

Параметры:

| Флаг | Описание |
| --- | --- |
| `-p, --process <json>` | параметры обработки (JSON), по умолчанию `default` |
| `--postprocess <json>` | параметры постобработки (JSON), по умолчанию выключено |
| `-o, --output <file.root>` | выходной файл (по умолчанию `<point>.root`) |

JSON-схема параметров соответствует serde-сериализации `ProcessParams` и
`PostProcessParams` из `numass-processing`.

В файл записываются: метаданные точки (`TString meta`) и дерево событий `T`
с ветками `time`, `channel`, `amplitude`, `size`.

## Интерактивный ROOT

Макросы в `testscripts/` (`rootlogon.C` подгружает `libnumass_root.so`).
После `rootlogon()` доступны `process(...)` и `get_meta(...)`.

## Структура

- `src/lib.rs` — Rust FFI-обвязка
- `bindings/processing.h` — C++ обёртка поверх сгенерированного заголовка
- `convert-to-root.cxx` — CLI
- `cmake/CPM.cmake` — пакетный менеджер зависимостей CPM
- `CMakeLists.txt` — сборка через CMake + Corrosion
