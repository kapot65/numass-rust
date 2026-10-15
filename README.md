# numass-rust

Монорепозиторий Rust-проектов для обработки numass-данных.

## Подпроекты

| Каталог | Назначение |
| --- | --- |
| [`dataforge-parser-rust`](dataforge-parser-rust/README.md) | Парсер протокола DataForge envelope |
| [`dataforge-parser-numass`](dataforge-parser-numass/README.md) | Типы numass-данных для DataForge |
| [`numass-processing`](numass-processing/README.md) | Общие алгоритмы обработки numass-точек |
| [`numass-analysis`](numass-analysis/README.md) | Скрипты обработки данных numass |
| [`numass-root`](numass-root/README.md) | C++-биндинги к CERN ROOT + утилита `convert-to-root`. Временно вне workspace |
| [`numass-server`](numass-server/README.md) | Сервер (кросс-компиляция) |
| [`numass-viewers`](numass-viewers/README.md) | Визуализация (wasm) |
| [`tqdc-controller-rust`](tqdc-controller-rust/README.md) | Контроллер TQDC (`frame-viewer`) |

Подробности — в `README.md` соответствующего подпроекта.

## Системные требования

### Общие (Rust-крейты)

- Rust (cargo)

### `numass-root` (дополнительно)

Крейт временно исключён из workspace: `cargo build --workspace` его не собирает. Каталог `numass-root/` на месте. Чтобы снова собрать, верните его в `members` в корневом `Cargo.toml`.

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

### `numass-server` (дополнительно)

Для сборки `numass-server` нужны:

- Rust (rustup) и WASM-таргет `wasm32-unknown-unknown`
- [Trunk](https://trunkrs.dev/) — сборка WASM-фронтенда (`cargo install trunk`)
- [Nushell](https://www.nushell.sh/) — для скрипта деплоя `update-web-viewer.nu`

Для кросс-компиляции (деплой на удалённую машину) дополнительно:

- [cross](https://github.com/cross-rs/cross) — кросс-компиляция (`cargo install cross`)
- Podman — контейнерный рантайм для `cross` (`sudo apt install podman`)

Обновить весь стек целиком одной командой:

```shell
rustup update && rustup target add wasm32-unknown-unknown && cargo install trunk
```

## Деплой `data-viewer-web`

Сборку и деплой выполняет скрипт [`update-web-viewer.nu`](update-web-viewer.nu):

- **Нативно** (на этой машине): сборка в `target/`, установка бинаря в `/usr/local/bin` и перезапуск systemd-сервиса `data-viewer-web`:

    ```shell
    nu update-web-viewer.nu
    ```

- **На удалённую машину** (кросс-компиляция под `x86_64-unknown-linux-gnu`): сборка в `target-cross/`, копирование бинаря по `scp` и перезапуск сервиса по `ssh`:

    ```shell
    nu update-web-viewer.nu 192.168.111.1
    ```
