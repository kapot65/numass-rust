#![cfg_attr(not(target_arch = "wasm32"), allow(unused))]
//! Отдельный wasm-воркер для исполнения preprocess-скриптов.
//!
//! Lua-throw в ulua — паника, а на wasm32-unknown-unknown (panic=abort)
//! паники не перехватываются: в главном модуле любой `error()` в скрипте
//! уронил бы всего интерфейса. Здесь воркер расходуем: trap убивает лишь
//! этот wasm-экземпляр, а главный модуль принимает это как `Err` (onerror)
//! и показывает ошибку, продолжая работать.

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    panic!("this binary is not meant to be run on desktop")
}

#[cfg(target_arch = "wasm32")]
fn main() {
    console_error_panic_hook::set_once();
    numass_viewers::script_worker::run_script_worker();
}
