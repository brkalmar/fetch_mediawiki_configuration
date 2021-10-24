use crate::pcre;
use itertools::Itertools;
use pcre::HirExt;
use regex_syntax::hir;
use serde::Deserialize;
use std::{collections, convert, env, error, fmt, iter};

const LINK_TRAIL_GROUP_INDEX: u32 = 1;

pub(crate) struct Endpoint {
    client: reqwest::blocking::Client,
    url: url::Url,
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
pub(crate) struct ExtensionTag(String);

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct General {
    linktrail: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(crate) struct MagicWord {
    aliases: Vec<String>,
    case_sensitive: Option<bool>,
    name: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(crate) struct NamespaceAlias {
    id: i64,
    alias: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct Namespace {
    id: i64,
    name: String,
    canonical: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(transparent)]
pub(crate) struct Protocol(String);

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct Response {
    query: Option<Box<serde_json::value::RawValue>>,

    errors: Option<ResponseErrors>,
    warnings: Option<ResponseErrors>,
}

#[derive(Debug)]
pub(crate) enum NewError {
    Reqwest(reqwest::Error),
    Url(url::ParseError),
}

#[derive(Debug)]
pub(crate) enum MalformedError {
    ExtensionTag(String),
    Json(serde_json::Error),
    LinkTrailInvalidGroup(String),
    LinkTrailNoGroup(String),
    NoNamespace(String),
    NoQuery,
    PCRE(pcre::Error),
    Response(ResponseErrors),
}

#[derive(Debug, Deserialize)]
#[serde(transparent)]
pub(crate) struct ResponseErrors(Vec<ResponseError>);

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct ResponseError {
    code: String,
    data: Option<serde_json::Value>,
    module: String,
    text: String,
}

impl Endpoint {
    pub(crate) fn fetch(&self) -> Result<Response, reqwest::Error> {
        let response = self.fetch_response()?;
        log::info!("response status: {}", response.status());

        for name in [
            reqwest::header::CONNECTION,
            reqwest::header::CONTENT_ENCODING,
            reqwest::header::CONTENT_LENGTH,
            reqwest::header::CONTENT_TYPE,
            reqwest::header::SERVER,
            reqwest::header::HeaderName::from_static("mediawiki-api-error"),
        ] {
            log::debug!(
                "response header: {:?}: {:?}",
                name,
                response.headers().get(&name)
            );
        }

        response.json::<Response>()
    }

    fn fetch_response(&self) -> Result<reqwest::blocking::Response, reqwest::Error> {
        self.client
            .get(self.url.as_ref())
            .send()?
            .error_for_status()
    }

    pub(crate) fn new(domain: &str) -> Result<Self, NewError> {
        let client = Self::new_client()?;
        let url = Self::new_url(domain)?;
        log::debug!("url = {}", url);
        Ok(Self { client, url })
    }

    fn new_client() -> Result<reqwest::blocking::Client, reqwest::Error> {
        let user_agent = format!(
            "{}/{} ({})",
            clap::crate_name!(),
            clap::crate_version!(),
            clap::crate_authors!(", ")
        );
        log::debug!("user_agent = {:?}", user_agent);
        reqwest::blocking::Client::builder()
            .user_agent(user_agent)
            .https_only(true)
            .deflate(true)
            .gzip(true)
            .build()
    }

    fn new_url(domain: &str) -> Result<url::Url, url::ParseError> {
        const CATEGORIES: &[&str] = &[
            "extensiontags",
            "general",
            "magicwords",
            "namespacealiases",
            "namespaces",
            "protocols",
        ];
        let mut url = url::Url::parse_with_params(
            "https://example.org/w/api.php",
            [
                ("action", "query"),
                ("meta", "siteinfo"),
                ("siprop", &CATEGORIES.iter().format("|").to_string()),
                ("format", "json"),
                ("formatversion", "2"),
                ("errorformat", "plaintext"),
            ],
        )
        .unwrap();
        url.set_host(Some(domain))?;
        Ok(url)
    }
}

impl Query {
    pub(crate) fn namespace_all_names(
        &self,
        canonical: &str,
    ) -> Result<collections::BTreeSet<String>, MalformedError> {
        let namespace = self
            .namespaces
            .values()
            .find(|ns| ns.canonical.as_ref().map(AsRef::as_ref) == Some(canonical))
            .ok_or_else(|| MalformedError::NoNamespace(canonical.to_owned()))?;
        let aliases = self
            .namespacealiases
            .iter()
            .filter(|na| na.id == namespace.id);
        let names = aliases
            .map(|na| na.alias.as_str())
            .chain(iter::once(canonical))
            .chain(iter::once(namespace.name.as_str()))
            .map(str::to_lowercase);
        Ok(names.collect())
    }

    pub(crate) fn extension_tags(&self) -> Result<collections::BTreeSet<String>, MalformedError> {
        self.extensiontags
            .iter()
            .map(|et| {
                et.0.as_str()
                    .strip_prefix("<")
                    .and_then(|s| s.strip_suffix(">"))
                    .map(str::to_lowercase)
                    .ok_or(MalformedError::ExtensionTag(et.0.clone()))
            })
            .collect()
    }

    pub(crate) fn protocols(&self) -> collections::BTreeSet<String> {
        self.protocols.iter().map(|p| p.0.to_lowercase()).collect()
    }

    pub(crate) fn link_trail(&self) -> Result<collections::BTreeSet<char>, MalformedError> {
        use hir::HirKind::*;

        let pattern: pcre::Pattern = self
            .general
            .linktrail
            .parse()
            .map_err(MalformedError::PCRE)?;
        log::debug!("pattern = {:?}", pattern);

        let group = pattern
            .hir
            .find_group_index(LINK_TRAIL_GROUP_INDEX)
            .ok_or_else(|| MalformedError::LinkTrailNoGroup(self.general.linktrail.to_owned()))?;
        let repeated = match group.hir.kind() {
            Empty => Ok(None),
            Repetition(repetition) => Ok(Some(&repetition.hir)),
            Alternation(..) | Anchor(..) | Class(..) | Concat(..) | Group(..) | Literal(..)
            | WordBoundary(..) => Err(MalformedError::LinkTrailInvalidGroup(
                self.general.linktrail.to_owned(),
            )),
        }?;
        log::debug!("repeated = {:?}", repeated.map(|r| pcre::HirDebugAlt(r)));

        let mut characters = Default::default();
        if let Some(repeated) = repeated {
            Self::link_trail_characters(repeated, &mut characters).map_err(|_| {
                MalformedError::LinkTrailInvalidGroup(self.general.linktrail.clone())
            })?;
        }
        Ok(characters)
    }

    fn link_trail_characters(
        hir: &hir::Hir,
        characters: &mut collections::BTreeSet<char>,
    ) -> Result<(), ()> {
        use hir::HirKind::*;
        use hir::{Class, Literal};
        match hir.kind() {
            Alternation(hirs) => {
                for hir in hirs {
                    Self::link_trail_characters(hir, characters)?;
                }
                Ok(())
            }
            Class(class) => {
                match class {
                    Class::Bytes(bytes) => {
                        for range in bytes.iter() {
                            for b in range.start()..=range.end() {
                                debug_assert!(b.is_ascii());
                                characters.insert(b.into());
                            }
                        }
                    }
                    Class::Unicode(unicode) => {
                        for range in unicode.iter() {
                            for c in range.start()..=range.end() {
                                characters.insert(c);
                            }
                        }
                    }
                }
                Ok(())
            }
            Group(group) => Self::link_trail_characters(&group.hir, characters),
            Literal(literal) => {
                let c = match literal {
                    Literal::Byte(..) => unreachable!(),
                    Literal::Unicode(c) => *c,
                };
                characters.insert(c);
                Ok(())
            }
            Anchor(..) | Concat(..) | Empty | Repetition(..) | WordBoundary(..) => Err(()),
        }
    }

    pub(crate) fn magic_words(&self) -> collections::BTreeSet<String> {
        self.magicwords
            .iter()
            .flat_map(|mw| {
                mw.aliases
                    .iter()
                    .map(AsRef::as_ref)
                    .chain(iter::once(mw.name.as_str()))
            })
            .filter_map(|s| s.strip_prefix("__").and_then(|s| s.strip_suffix("__")))
            .map(str::to_lowercase)
            .collect()
    }

    pub(crate) fn magic_words_redirect(&self) -> collections::BTreeSet<String> {
        const NAME: &str = "redirect";
        const PREFIX: &str = "#";
        self.magicwords
            .iter()
            .filter(|mw| mw.name == NAME)
            .flat_map(|mw| mw.aliases.iter())
            .map(|s| s.strip_prefix(PREFIX).unwrap_or(s))
            .chain(iter::once(NAME))
            .map(str::to_lowercase)
            .collect()
    }
}

impl convert::TryFrom<Response> for Query {
    type Error = MalformedError;

    fn try_from(response: Response) -> Result<Self, Self::Error> {
        if let Some(errors) = response.errors {
            return Err(errors.into());
        }
        if let Some(warnings) = response.warnings {
            return Err(warnings.into());
        }
        serde_json::from_str(response.query.ok_or(MalformedError::NoQuery)?.get())
            .map_err(Into::into)
    }
}

impl fmt::Display for NewError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use NewError::*;
        match self {
            Reqwest(e) => write!(f, "{}", e),
            Url(e) => write!(f, "{}", e),
        }
    }
}

impl error::Error for NewError {}

impl From<reqwest::Error> for NewError {
    fn from(e: reqwest::Error) -> Self {
        Self::Reqwest(e)
    }
}

impl From<url::ParseError> for NewError {
    fn from(e: url::ParseError) -> Self {
        Self::Url(e)
    }
}

impl fmt::Display for MalformedError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use MalformedError::*;

        write!(f, "malformed API response: ")?;
        match self {
            ExtensionTag(tag) => write!(f, "extension tag not of the form `<...>`: {:?}", tag)?,
            Json(e) => write!(f, "{}", e)?,
            LinkTrailInvalidGroup(pattern) => write!(
                f,
                "structure of group {} in link trail pattern: {:?}",
                LINK_TRAIL_GROUP_INDEX, pattern
            )?,
            LinkTrailNoGroup(pattern) => write!(
                f,
                "no group {} in link trail pattern: {:?}",
                LINK_TRAIL_GROUP_INDEX, pattern
            )?,
            NoNamespace(name) => write!(f, "no namespace {:?}", name)?,
            NoQuery => write!(f, "no errors or warnings, and no query")?,
            PCRE(e) => write!(f, "{}", e)?,
            Response(e) => write!(f, "{}", e)?,
        }
        Ok(())
    }
}

impl error::Error for MalformedError {}

impl From<serde_json::Error> for MalformedError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

impl From<ResponseErrors> for MalformedError {
    fn from(e: ResponseErrors) -> Self {
        Self::Response(e)
    }
}

impl fmt::Display for ResponseErrors {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0.iter().format("; "))
    }
}

impl error::Error for ResponseErrors {}

impl fmt::Display for ResponseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "API error: [{}] {} {}",
            self.module, self.code, self.text
        )?;
        if let Some(ref data) = self.data {
            write!(f, " ({:?})", data)?;
        }
        Ok(())
    }
}
