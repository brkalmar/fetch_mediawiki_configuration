use err_derive::Error;
use itertools::Itertools;
use serde::Deserialize;
use std::{collections, error, fmt};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct Response {
    pub(super) query: Option<Box<serde_json::value::RawValue>>,

    pub(super) errors: Option<Errors>,
    pub(super) warnings: Option<Errors>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(crate) struct Query {
    pub(crate) extensiontags: Vec<ExtensionTag>,
    pub(crate) general: General,
    pub(crate) magicwords: Vec<MagicWord>,
    pub(crate) namespacealiases: Vec<NamespaceAlias>,
    pub(crate) namespaces: collections::BTreeMap<String, Namespace>,
    pub(crate) protocols: Vec<Protocol>,
}

#[derive(Debug, Deserialize)]
#[serde(transparent)]
pub(crate) struct ExtensionTag(pub(crate) String);

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct General {
    pub(crate) linktrail: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(crate) struct MagicWord {
    pub(crate) aliases: Vec<String>,
    pub(crate) case_sensitive: Option<bool>,
    pub(crate) name: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(crate) struct NamespaceAlias {
    pub(crate) id: i64,
    pub(crate) alias: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct Namespace {
    pub(crate) id: i64,
    pub(crate) name: String,
    pub(crate) canonical: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(transparent)]
pub(crate) struct Protocol(pub(crate) String);

#[derive(Debug, Deserialize)]
#[serde(transparent)]
pub(crate) struct Errors(pub(crate) Vec<Error>);

#[derive(Debug, Deserialize, Error)]
#[error(display = "API error: [{}] {} {} ({:?})", module, code, text, data)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(crate) struct Error {
    code: String,
    data: Option<serde_json::Value>,
    module: String,
    text: String,
}

impl fmt::Display for Errors {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0.iter().format("; "))
    }
}

impl error::Error for Errors {}
