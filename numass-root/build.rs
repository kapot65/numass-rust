extern crate cbindgen;

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

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
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR is not set"));
    // OUT_DIR = <target>/<profile>/build/<crate>-<hash>/out, поэтому каталог
    // профиля (<target>/<profile>) — четвёртый предок.
    let profile_dir = out_dir
        .ancestors()
        .nth(3)
        .expect("Failed to resolve cargo profile directory")
        .to_path_buf();

    let profile = env::var("PROFILE").unwrap_or_else(|_| "release".to_string());
    let build_type = if profile == "debug" {
        "Debug"
    } else {
        "Release"
    };
    let build_dir = out_dir.join("cmake-build");

    let mut configure = Command::new("cmake");
    configure
        .arg("-S")
        .arg(crate_dir)
        .arg("-B")
        .arg(&build_dir)
        .arg(format!("-DCMAKE_BUILD_TYPE={build_type}"))
        .arg(format!(
            "-DCMAKE_RUNTIME_OUTPUT_DIRECTORY={}",
            profile_dir.display()
        ))
        .env(INNER_BUILD_ENV, "1");

    if let Some(prefix) = cmake_prefix_path() {
        configure.arg(format!("-DCMAKE_PREFIX_PATH={prefix}"));
    }

    run(&mut configure);

    run(Command::new("cmake")
        .arg("--build")
        .arg(&build_dir)
        .arg("-j")
        .env(INNER_BUILD_ENV, "1"));
}

/// Возвращает `CMAKE_PREFIX_PATH` для CMake: унаследованный из окружения путь
/// плюс локально собранный Vdt (зависимость ROOT), если он обнаруживается.
fn cmake_prefix_path() -> Option<String> {
    let mut prefixes: Vec<String> = env::var("CMAKE_PREFIX_PATH")
        .unwrap_or_default()
        .split(':')
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();

    for candidate in vdt_candidates() {
        if is_vdt_prefix(&candidate) {
            let path = candidate.to_string_lossy().to_string();
            if !prefixes.contains(&path) {
                prefixes.push(path);
            }
        }
    }

    if prefixes.is_empty() {
        None
    } else {
        Some(prefixes.join(":"))
    }
}

fn vdt_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(dir) = env::var("Vdt_DIR") {
        candidates.push(PathBuf::from(dir));
    }
    // Vdt обычно собирают рядом с ROOT.
    if let Ok(rootsys) = env::var("ROOTSYS") {
        candidates.push(PathBuf::from(&rootsys).join("..").join("vdt"));
    }
    if let Ok(home) = env::var("HOME") {
        candidates.push(PathBuf::from(home).join("apps").join("vdt"));
    }

    candidates
}

fn is_vdt_prefix(prefix: &Path) -> bool {
    prefix.join("lib/libvdt.so").is_file() || prefix.join("libvdt.so").is_file()
}

fn run(cmd: &mut Command) {
    let status = cmd.status().expect("failed to spawn command");
    if !status.success() {
        panic!("command failed: {cmd:?}");
    }
}
