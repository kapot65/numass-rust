use std::process::Command;

fn main() {
    let output = Command::new("git")
        .args(&["describe", "--tags", "--always", "--dirty"])
        .output()
        .expect("Failed to execute git describe");

    let git_version = String::from_utf8(output.stdout)
        .expect("Invalid UTF-8 in git describe output")
        .trim()
        .to_string();

    // Устанавливаем переменную окружения для использования в коде
    // println!("cargo:rustc-env=GIT_VERSION={}", git_version);

    // Обновляем версию для Cargo (опционально, для отображения в cargo)
    println!("cargo:rustc-env=CARGO_PKG_VERSION={}", git_version);
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs");
}