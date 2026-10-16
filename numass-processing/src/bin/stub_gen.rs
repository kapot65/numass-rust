//! Генерирует `numass_processing.pyi` из аннотаций `gen_stub_*`.
//!
//! Запуск: `cargo run -p numass-processing --features stubgen --bin stub_gen`.

fn main() -> pyo3_stub_gen::Result<()> {
    numass_processing::stub_info()?.generate()?;
    Ok(())
}
