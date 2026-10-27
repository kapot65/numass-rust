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
        }
    }
}

impl std::error::Error for PipelineError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Script(err) => Some(err),
            Self::Data(_) => None,
        }
    }
}

impl From<ScriptError> for PipelineError {
    fn from(err: ScriptError) -> Self {
        Self::Script(err)
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
}
