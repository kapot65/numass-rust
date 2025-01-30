### Порядок действий по калибровке
1. Набрать усредненные спектры без калибровки
    - выставить в [calibrate-with-tritium.rs](https://bitbucket.org/Kapot/tqdc-controller-rust/src/new-tqdc/analysis/src/bin/calibrate-with-tritium.rs) следующие параметры
        ```RUST
        let u_sp = [12000, 12500, 13000, 13500, 14000, 14500, 15000, 15500, 16000, 16500, 17000];
 
        let processing_params = ProcessParams {
            algorithm: Algorithm::default(),
            convert_to_kev: false,
        };

        let hist = PointHistogram::new(0.0..120.0, 480);
        ```
    - запустить `cargo run --release --bin calibrate-with-tritium`
    - скопировать полученные файлы в папку `raw`

2. Набрать усредненные спектры с калибровкой (по Electrode_2)
    - выставить в [numass-processing/src/lib.rs](https://bitbucket.org/Kapot/numass-processing/src/master/src/lib.rs) следующие параметры
        ```RUST
        // calibration by Electrode_2
        const KEV_COEFF_FIRST_PEAK: [[f32; 2]; 7] = [
            [0.30209273, -0.022],
            [0.25891086, -0.0007972717],
            [0.2746626, -0.036146164],
            [0.27816013, 0.081],
            [0.28441244, -0.0133],
            [0.27044022, -0.01026],
            [0.2477852, -0.0318],
        ];
        ```
    - (вернуть исходную калибровку) выставить в выставить в [calibrate-with-tritium.rs](https://bitbucket.org/Kapot/tqdc-controller-rust/src/new-tqdc/analysis/src/bin/calibrate-with-tritium.rs)
        ```RUST
        let u_sp = [12000, 12500, 13000, 13500, 14000, 14500, 15000, 15500, 16000, 16500, 17000];

        let processing_params = ProcessParams {
            algorithm: Algorithm::default(),
            convert_to_kev: true,
        };

        let hist = PointHistogram::new(0.0..20.0, 400);
        ```
    - запустить `cargo run --release --bin calibrate-with-tritium`
    - скопировать полученные файлы в папку `kev`
3. рассчитать фиты для сырых данных
    - запустить `root 'fit.C("raw", 1.0, 2.0)' --web=off`
    - проверить (визуально) что фиты сошлись
4. рассчитать фиты для калиброванных данных
    - запустить `root 'fit.C("kev", 0.3, 0.8)' --web=off`
    - проверить (визуально) что фиты сошлись
5. рассчитать калибровку
    - запустить `root 'calibrate.C' --web=off`
    - проверить (визуально) что калибровка сошлась
6. запихнуть полученную калибровку из [coeffs.json](./output/coeffs.json) в [numass-processing/src/lib.rs](https://bitbucket.org/Kapot/numass-processing/src/master/src/lib.rs)
7. оценить визуально качество калибровки с помощью data-viewer
    ```BASH
    # запускать из numass-viewers
    cargo run --release --bin data-viewer -- --directory=/data-nvme/2023_03/
    ```