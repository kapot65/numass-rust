//! Ctrl+C в скриптовом редакторе на wasm.
//!
//! Eframe на web копирует выделение через асинхронную запись
//! `navigator.clipboard.writeText`, которую браузеры блокируют вне secure
//! контекстов и при ужесточённых политиках (emilk/egui#3480, #4213,
//! discussion #4462); родной copy-event при этом preventDefault'ится
//! `WebOptions` по умолчанию, так что копирование выделения DOM тоже не
//! работает. Поэтому Ctrl+C мы обрабатываем сами: скрытый textarea плюс
//! `document.execCommand("copy")` — синхронный путь внутри user-gesture,
//! не требующий разрешения на запись в буфер.
//!
//! Текст сфокусированного редактора публикуется каждый кадр
//! (`publish_script_selection`); document-level keydown-обработчик
//! перехватывает Ctrl+C / Cmd+C только при непустом выделении, так что
//! на остальные сочетания он не влияет.

use std::cell::RefCell;
use std::sync::OnceLock;

use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

thread_local! {
    /// Выделение сфокусированного скриптового редактора в текущем кадре.
    /// Пустая строка — выделения нет: Ctrl+C не перехватывается.
    static SCRIPT_SELECTION: RefCell<String> = const { RefCell::new(String::new()) };
}

/// Публикует текст выделения сфокусированного скриптового редактора.
/// Редактор зовёт это на каждом кадре — с выделением или пустой строкой,
/// чтобы при потере фокуса мост не копировал устаревший текст.
pub fn publish_script_selection(text: &str) {
    SCRIPT_SELECTION.with(|slot| *slot.borrow_mut() = text.to_owned());
}

/// Ставит document-level keydown-обработчик Ctrl+C / Cmd+C (capture — раньше
/// слушателей eframe на canvas и у текстового агента). Повторные вызовы — no-op.
pub fn ensure_shortcut_installed() {
    static INSTALLED: OnceLock<()> = OnceLock::new();
    if INSTALLED.set(()).is_err() {
        return;
    }

    let Some(document) = document() else {
        return;
    };
    let handler = Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(handle_keydown);
    if let Err(err) = document.add_event_listener_with_callback_and_bool(
        "keydown",
        handler.as_ref().unchecked_ref(),
        true,
    ) {
        web_sys::console::warn_1(
            &format!("numass: failed to install Ctrl+C bridge: {err:?}").into(),
        );
        return;
    }
    // Слушатель живёт до конца страницы, замыкание не освобождаем.
    handler.forget();
}

fn document() -> Option<web_sys::Document> {
    web_sys::window()?.document()
}

fn handle_keydown(event: web_sys::KeyboardEvent) {
    // Физическая клавиша C: не зависит от раскладки (Ctrl+Ц на русской).
    if event.code() != "KeyC" || !(event.ctrl_key() || event.meta_key()) {
        return;
    }
    if event.alt_key() || event.shift_key() {
        return;
    }

    let selection = SCRIPT_SELECTION.with(|slot| slot.borrow().clone());
    if selection.is_empty() {
        return; // выделения в скриптовом редакторе нет — не перехватываем
    }

    // Полный перехват: egui и eframe не видят ни keydown, ни родной copy-event.
    event.prevent_default();
    event.stop_propagation();

    copy_selection_to_clipboard(&selection);
}

fn copy_selection_to_clipboard(text: &str) {
    let Some(document) = document() else {
        return;
    };
    let Some(textarea) = document
        .create_element("textarea")
        .ok()
        .and_then(|element| element.dyn_into::<web_sys::HtmlTextAreaElement>().ok())
    else {
        return;
    };

    // Одноразовый capture-щит: exec_command("copy") dispatch'ит вложенный
    // copy-event, а слушатель eframe вызывает preventDefault и отменяет
    // копирование. Наш capture-слушатель обрубает dispatch до слушателя
    // eframe и не трогает default-действие (саму запись в буфер).
    let shield = Closure::<dyn FnMut(web_sys::Event)>::new(|event: web_sys::Event| {
        event.stop_propagation();
    });
    let _ = document.add_event_listener_with_callback_and_bool(
        "copy",
        shield.as_ref().unchecked_ref(),
        true,
    );

    let style = textarea.style();
    let _ = style.set_property("position", "fixed");
    let _ = style.set_property("left", "-9999px");
    let _ = style.set_property("top", "0");
    textarea.set_value(text);
    if let Some(body) = document.body() {
        let _ = body.append_child(&textarea);
    }

    let _ = textarea.focus();
    textarea.select();

    let copied = document
        .unchecked_ref::<web_sys::HtmlDocument>()
        .exec_command("copy")
        .unwrap_or(false);

    // Фокус — обратно канвасу, чтобы клавиатура продолжила попадать в egui.
    if let Some(canvas) = document.query_selector("canvas").ok().flatten() {
        let _ = canvas.unchecked_ref::<web_sys::HtmlElement>().focus();
    }
    textarea.remove();
    let _ = document.remove_event_listener_with_callback_and_bool(
        "copy",
        shield.as_ref().unchecked_ref(),
        true,
    );

    if !copied {
        web_sys::console::warn_1(&"numass: Ctrl+C: execCommand(\"copy\") failed".into());
    }
}
