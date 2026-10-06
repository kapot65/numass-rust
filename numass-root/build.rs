extern crate cbindgen;

use std::env;
use std::path::PathBuf;

/// Выставляется в окружении при запуске CMake. Вложенная сборка через
/// Corrosion (cargo -> cmake -> cargo) обязана пропустить вызов CMake,
/// иначе получится бесконечная рекурсия.
const INNER_BUILD_ENV: &str = "NUMASS_ROOT_CMAKE";

fn main() {
    if env::var(INNER_BUILD_ENV).is_ok() {
        return;
    }

    let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();

    cbindgen::Builder::new()
        .with_crate(&crate_dir)
        .with_config(
            cbindgen::Config::from_file("cbindgen.toml").expect("Can't find/parse cbindgen.toml"),
        )
        .generate()
        .expect("Unable to generate bindings")
        .write_to_file("./bindings/processing-internal.h");

    run_cmake(&crate_dir);
}

fn run_cmake(crate_dir: &str) {
    let mut config = cmake::Config::new(crate_dir);
    // Крейт по умолчанию собирает цель install, а в CMakeLists нет правил
    // install(). Без цели это прежний `cmake --build` (all).
    // CMAKE_BUILD_TYPE и --parallel крейт берёт из профиля Cargo.
    config
        .no_build_target(true)
        .define("CMAKE_RUNTIME_OUTPUT_DIRECTORY", cargo_profile_dir())
        .env(INNER_BUILD_ENV, "1");

    config.build();
}

/// Каталог профиля Cargo (`target/debug` или `target/release`), куда кладётся
/// `convert-to-root`.
fn cargo_profile_dir() -> PathBuf {
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR is not set"));
    // OUT_DIR = <target>/<profile>/build/<crate>-<hash>/out, поэтому каталог
    // профиля (<target>/<profile>) — четвёртый предок.
    out_dir
        .ancestors()
        .nth(3)
        .expect("Failed to resolve cargo profile directory")
        .to_path_buf()
}
