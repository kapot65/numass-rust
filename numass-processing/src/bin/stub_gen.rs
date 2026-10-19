//! Генерирует `numass_processing.pyi` из аннотаций `gen_stub_*` и правит его
//! линтером.
//!
//! Запуск: `cargo run -p numass-processing --features stubgen --bin stub_gen`.
//!
//! После генерации запускается `ruff check --fix`. Если `ruff` нет в `PATH`,
//! используется `uvx ruff`: так шаг работает и без глобально установленного
//! линтера.

use std::path::Path;
use std::process::Command;

use pyo3_stub_gen::Result;

const STUB_FILE: &str = "numass_processing.pyi";

fn main() -> Result<()> {
    let package_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let stub_path = package_dir.join(STUB_FILE);

    numass_processing::stub_info()?.generate()?;
    fix_with_ruff(&stub_path, package_dir)?;

    Ok(())
}

/// Прогоняет `ruff check --fix` по сгенерированному стабу.
///
/// Первая найденная команда выигрывает: `ruff`, затем `uvx ruff`, затем
/// `uv tool run ruff`. Ненулевой код возврата означает, что остались
/// неисправимые замечания, и генерация завершается ошибкой.
fn fix_with_ruff(stub_path: &Path, package_dir: &Path) -> std::io::Result<()> {
    let candidates: &[(&str, &[&str])] = &[
        ("ruff", &["check", "--fix", "--output-format=concise"]),
        (
            "uvx",
            &["ruff", "check", "--fix", "--output-format=concise"],
        ),
        (
            "uv",
            &[
                "tool",
                "run",
                "ruff",
                "check",
                "--fix",
                "--output-format=concise",
            ],
        ),
    ];

    for (program, args) in candidates {
        match Command::new(program)
            .args(*args)
            .arg(stub_path)
            .current_dir(package_dir)
            .status()
        {
            Ok(status) if status.success() => return Ok(()),
            Ok(status) => {
                return Err(std::io::Error::other(format!(
                    "ruff exited with {status}: generated stub still has diagnostics"
                )));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        }
    }

    Err(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "ruff not found: install `ruff` or make `uvx` available",
    ))
}
