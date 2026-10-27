//! Тип ошибки пайплайна обработки: preprocess → process → postprocess.
//!
//! Один тип протаскивается наверх через `?`:
//! - вьюверы показывают [`Display`](std::fmt::Display) пользователю;
//! - Python-биндинги превращают в исключение;
//! - batch-скрипты и остальная no-GUI обработка делают `unwrap` — паника,
//!   как раньше.
//!
//! Не в этом типе: ошибки сети/файловой системы при чтении байтов
//! ([`crate::storage::read_range`]) и разбор протоколы точки — они не относятся
//! к шагам пайплайна.

use std::fmt;

use crate::script::ScriptError;

/// Ошибка любого шага пайплайна обработки.
#[derive(Debug)]
pub enum PipelineError {
    /// Ошибка компиляции (парсинга) или исполнения preprocess-скрипта.
    ///
    /// Сообщения ulua (`syntax error: …`, `runtime error: …`) сами по себе
    /// достаточно описывают источник для пользователя.
    Script(ScriptError),
    /// Данные события/кадра несовместимы с пайплайном: канал вне калибровки,
    /// кадр короче окна алгоритма и т.п. Возникает как из сломанной точки,
    /// так и из скрипта, укоротившего или заполнившего кадр браком.
    Data(String),
    /// Точка не загрузилась из файла: битый заголовок DataForge или
    /// неопарсящийся protobuf.
    Load(LoadPointError),
}

impl PipelineError {
    /// Ошибка данных с описанием: значение, канал, ожидание.
    #[must_use]
    pub fn data(message: impl Into<String>) -> Self {
        Self::Data(message.into())
    }
}

impl fmt::Display for PipelineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // Текст совместим с прежней паникой шага (`preprocess script: {err}`).
            Self::Script(err) => write!(f, "preprocess script: {err}"),
            Self::Data(message) => write!(f, "{message}"),
            Self::Load(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for PipelineError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Script(err) => Some(err),
            Self::Data(_) => None,
            Self::Load(err) => Some(err),
        }
    }
}

impl From<ScriptError> for PipelineError {
    fn from(err: ScriptError) -> Self {
        Self::Script(err)
    }
}

/// Точка не разборалась/не загрузилась из хранилища.
#[derive(Debug)]
pub enum LoadPointError {
    /// Файл не открылся/не читается (только native).
    Io(std::io::Error),
    /// Битый заголовок DF01 (заголовок/мета не читаются).
    DataForge(dataforge::DFParseError),
    /// Данные точки не разобрались как protobuf (`rsb_event::Point`).
    Proto(protobuf::Error),
    /// Сбой HTTP-загрузки/неожиданный ответ (только wasm: fetch, статус,
    /// undecodable-тело ответа).
    Http(String),
}

impl fmt::Display for LoadPointError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "cannot read point file: {err}"),
            Self::DataForge(err) => write!(f, "broken dataforge header: {err}"),
            Self::Proto(err) => write!(f, "broken point data: {err}"),
            Self::Http(message) => write!(f, "cannot load point over http: {message}"),
        }
    }
}

impl std::error::Error for LoadPointError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            Self::DataForge(err) => Some(err),
            Self::Proto(err) => Some(err),
            Self::Http(_) => None,
        }
    }
}

impl From<LoadPointError> for PipelineError {
    fn from(err: LoadPointError) -> Self {
        Self::Load(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error as _;

    /// [`PipelineError::Script`] сохраняет текст прежней паники шага.
    #[test]
    fn script_display_keeps_panic_text() {
        let err = PipelineError::Script(ScriptError::RuntimeError("boom".to_owned()));
        assert_eq!(err.to_string(), "preprocess script: runtime error: boom");
    }

    #[test]
    fn data_message_is_bare() {
        let err = PipelineError::data("channel 9 is out of the kev calibration");
        assert!(err.source().is_none());
        assert_eq!(err.to_string(), "channel 9 is out of the kev calibration");
    }

    // LoadPointError::DataForge — битый заголовок DF файла.
    #[test]
    fn load_error_display_and_source() {
        let garbage = [0xffu8; 30];
        let header_err =
            dataforge::read_binary_header_sync(&mut std::io::Cursor::new(garbage.as_slice()))
                .expect_err("garbage header must fail");
        let err = PipelineError::Load(LoadPointError::DataForge(header_err));
        assert!(err.source().is_some());
        assert!(
            err.to_string().starts_with("broken dataforge header"),
            "{}",
            err
        );
    }

    #[test]
    fn transport_error_display() {
        let err = LoadPointError::Http("process/hist: HTTP 500: boom".to_owned());
        assert!(err.source().is_none());
        assert!(
            err.to_string().starts_with("cannot load point over http"),
            "{}",
            err
        );
    }
}
