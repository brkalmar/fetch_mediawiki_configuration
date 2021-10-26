use err_derive::Error;
use itertools::Itertools;
use serde::Deserialize;
use std::{collections, error, fmt};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Response {
    pub query: Option<Box<serde_json::value::RawValue>>,

    pub errors: Option<Errors>,
    pub warnings: Option<Errors>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Query {
    pub extensiontags: Vec<ExtensionTag>,
    pub general: General,
    pub magicwords: Vec<MagicWord>,
    pub namespacealiases: Vec<NamespaceAlias>,
    pub namespaces: collections::BTreeMap<String, Namespace>,
    pub protocols: Vec<Protocol>,
}

#[derive(Debug, Deserialize)]
#[serde(transparent)]
pub struct ExtensionTag(pub String);

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct General {
    pub linktrail: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct MagicWord {
    pub aliases: Vec<String>,
    pub case_sensitive: Option<bool>,
    pub name: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct NamespaceAlias {
    pub id: i64,
    pub alias: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Namespace {
    pub id: i64,
    pub name: String,
    pub canonical: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(transparent)]
pub struct Protocol(pub String);

#[derive(Debug, Deserialize)]
#[serde(transparent)]
pub struct Errors(pub Vec<Error>);

#[derive(Debug, Deserialize, Error)]
#[error(display = "siteinfo API [{}] {} {} ({:?})", module, code, text, data)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Error {
    pub code: String,
    pub data: Option<serde_json::Value>,
    pub module: String,
    pub text: String,
}

impl fmt::Display for Errors {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0.iter().format("; "))
    }
}

impl error::Error for Errors {}
