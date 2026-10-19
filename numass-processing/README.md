This repository contains common numass processing algorithms used in 2022_12 run

[Получение амплитуды из осциллограммы (18.02.2025)](docs/waveform-to-amp.md)

## Python

Модуль `numass_processing` собирается фичей `python` (вместе с ней включается `polars`).

```shell
maturin develop --manifest-path numass-processing/Cargo.toml
```

Стабы генерируются отдельным шагом; сгенерированный `numass_processing.pyi` автоматически правится `ruff check --fix` (если `ruff` нет в `PATH`, используется `uvx ruff`).

```shell
cargo run -p numass-processing --features stubgen --bin stub_gen
```

Шаги обработки перенесены один в один из Rust: те же аргументы, тот же результат. Результат — Rust-структура. Отдельные биндинги полей промежуточных типов не пишутся; если доступ к полю появляется сам, это нормально. Промежуточные значения не копируются: объект Python держит исходные данные, и шаг читает их так же, как Rust.

У результата обработки есть метод `to_polars()`. Это одностороннее преобразование в таблицу Polars. Таблицу можно собирать как угодно. После `to_polars()` это `polars.DataFrame`: методы обработки модуля на нём уже нельзя вызвать.

Сценарий: собрать цепь предобработка → (обработка) → (постобработка), вызвать `to_polars()`, дальше анализировать таблицу на Python. Пример предобработки точки: `examples/preprocess.py`.

### convert-to-root

Скрипт ставится из этого каталога. Extra `root` ставит uproot, awkward и pyarrow. Пакет ROOT с PyPI не нужен.

```shell
cd numass-processing
uv tool install '.[root]'
```

`uv tool install` не принимает `--extra`: extra задаётся как `.[root]`. Повторная установка после правок: `uv tool install --force '.[root]'`. Команда попадает в `PATH` как `convert-to-root`.

Без глобальной установки, из каталога пакета:

```shell
uv run --extra root python convert-to-root.py /path/to/point -o point.root
```

Вызов:

```shell
convert-to-root /path/to/point -o point.root
```

Без флагов в файл пишется сырая точка. `--preprocess` добавляет предобработку, вместе с `-p`/`--process` — события, вместе с `--postprocess` — полную цепь. `--process` без `--preprocess` и `--postprocess` без `--process` завершаются с ошибкой. Значение флага — JSON serde-структуры `PreprocessParams`, `ProcessParams` или `PostProcessParams`. Ошибку разбора печатает Rust. Выход по умолчанию — `<имя файла точки>.root` в текущем каталоге.

```shell
convert-to-root /path/to/point --preprocess '{"steps":[]}' \
  -p '{"algorithm":{"Max":{}},"convert_to_kev":true}'
```

В файле один RNTuple `T`. Колонки те же, что у таблицы Polars этого шага. Метаданные в файл не пишутся.

<!-- TODO: расписать типы обработки (по точке, по кадру) -->

<!-- расписать по #[cfg(feature = "egui")] ui: &mut Option<&mut PlotUi>, -->