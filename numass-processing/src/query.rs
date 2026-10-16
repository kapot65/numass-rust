//! Query `params=<json>` для GET `/api` и режима вьювера.
//!
//! Голый `serde_qs` выкидывает пустые варианты (`Average`, `Invert`),
//! поэтому JSON передаётся одним значением. `serde_qs` только кодирует это значение.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct ParamsQuery {
    params: String,
}

pub(crate) fn to_query_string(value: &impl Serialize) -> String {
    serde_qs::to_string(&ParamsQuery {
        params: serde_json::to_string(value).expect("params value is JSON"),
    })
    .expect("params query string")
}

pub(crate) fn from_query_string<T: DeserializeOwned>(query: &str) -> Result<T, String> {
    let wrapper: ParamsQuery = serde_qs::from_str(query).map_err(|err| err.to_string())?;
    serde_json::from_str(&wrapper.params).map_err(|err| err.to_string())
}
