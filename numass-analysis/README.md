скрипты обработки данных numass

pkg-config
cmake
libfontconfig-dev

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