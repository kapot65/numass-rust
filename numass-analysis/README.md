скрипты обработки данных numass

pkg-config
cmake
libfontconfig-dev

## C++/ROOT-скрипты

C++/ROOT часть собирается отдельно от cargo-воркспейса через CMake. Бинарники
создаются автоматически: каждый файл `scripts/**/*.C` становится исполняемым
файлом с именем этого файла.

Требования: `cmake >= 3.24`, CERN ROOT (`>= 6.16`, `root-config` в `PATH`),
компилятор с поддержкой стандарта, с которым собран ROOT (стандарт берётся из
`ROOT_CXX_STANDARD`). CLI11 и nlohmann_json ставятся пакетным менеджером
[CPM](https://github.com/cpm-cmake/CPM.cmake), сам CPM скачивается при
конфигурации — в репозитории его код не хранится.

### Сборка

```sh
cmake -S numass-analysis -B numass-analysis/build
cmake --build numass-analysis/build -j
```

Готовые бинарники:

| Скрипт | Бинарник | Аргументы |
| --- | --- | --- |
| `scripts/fit-deltas.C` | `fit-deltas` | нет |
| `scripts/recalibration/fit.C` | `fit` | `<folder> <left> <right>` |
| `scripts/recalibration/calibrate.C` | `calibrate` | нет |

Бинарники лежат в `numass-analysis/build/bin`.

### Запуск

Скрипты читают файлы по относительным путям, поэтому запускать их нужно из
каталога с данными:

```sh
cd numass-analysis/scripts/recalibration

# вместо root 'fit.C+("raw", 1.0, 2.0)'
../../build/bin/fit raw 1.0 2.0

# читает ./output/calibration_raw.csv и ./output/calibration_kev.csv,
# пишет ./output/coeffs.json
../../build/bin/calibrate
```

`fit-deltas` ожидает `11kev-deltas-all.tsv` в текущем каталоге (см. комментарий
в начале `scripts/fit-deltas.C`):

```sh
numass-analysis/build/bin/fit-deltas
```

### Новый скрипт

Достаточно положить файл в `scripts/` — CMake подхватит его сам
(`CONFIGURE_DEPENDS`) при следующей сборке. Соглашение:

- функция называется как файл, где `-` заменён на `_`
  (`foo-bar.C` → `foo_bar()`);
- допустимые сигнатуры: `void foo_bar()` либо
  `foo_bar(const char* folder, double left, double right)` — во втором случае
  аргументы берутся из командной строки.

Для clangd/zed CMake пишет `build/compile_commands.json` — он нужен для
навигации по ROOT-заголовкам.

## Коррекция на мониторные точки
1. Получить коэффициенты мониторовных точек.
   1. Преобразовать в формат типа (можно через [convert-monitors.jl](scripts/convert-monitors.jl))
    ```json
    {
        "Tritium_1": {
            "set_1": {
            "corr_coef": [
                -6.56422177e-10,  8.72626826e-04, -2.89010123e+02
            ]
            },
            "set_2": {
            "corr_coef": [
                    -5.67519438e-10,  7.55296290e-04, -2.50290767e+02
                ]
            }
        }
    }
    ```
   2. Сверить тип коэффициентов
    - для мониторов по точкам оставить `coeffs.get_by_index(&filepath);` в [check-monitoring.rs](src/bin/check-monitoring.rs) и [produce-data-cli.rs](src/bin/produce-data-cli.rs)
    - для мониторов по абсолютному времени поменять на `coeffs.get_from_meta(&filepath, &meta);`
   3. Выставить констаны в [check-monitoring.rs](src/bin/check-monitoring.rs)
   4. Визуально проверить корректность поправки
      ```
      cargo run --release --bin check-monitoring
      ```
      либо по одному филу, либо все сразу ([поддерживаемые wildcard паттерны](https://docs.rs/glob/0.3.2/glob/struct.Pattern.html)).

## Основная обработка
[produce-data-cli.rs](src/bin/produce-data-cli.rs)
1. Создать конфигурационный файл на основе [sample.yaml](resources/produce-data-cli-config-sample.yaml)
    Для получения значений process и postprocess по-умлочанию можно использовать эти сниппеты:
    ```rust
    #[test]
    fn serialize_default_process(){
        println!(
            "{}",
            serde_yaml::to_string(&processing::process::ProcessParams::default()).unwrap()
        );
    }

    #[test]
    fn serialize_default_postprocess(){
        println!(
            "{}",
            serde_yaml::to_string(&processing::postprocess::PostProcessParams::default()).unwrap()
        );
    }
    ```
2. Поправить [produce-data-cli.rs](src/bin/produce-data-cli.rs) под текущие требования.
3. 


## TODO
- [ ] задавать тип мониторных точек через enum