use crate::pcre;
use err_derive::Error;
use itertools::Itertools;
use pcre::HirExt;
use regex_syntax::hir;
use std::{collections, convert, env, iter};

pub(crate) mod response;

const LINK_TRAIL_GROUP_INDEX: u32 = 1;

pub(crate) struct Endpoint {
    client: reqwest::blocking::Client,
    url: url::Url,
}

#[derive(Debug, Error)]
pub(crate) enum NewError {
    #[error(display = "{}", _0)]
    Reqwest(#[error(source)] reqwest::Error),
    #[error(display = "{}", _0)]
    Url(#[error(source)] url::ParseError),
}

#[derive(Debug, Error)]
pub(crate) enum MalformedError {
    #[error(
        display = "malformed API response: extension tag not of the form `<...>`: {:?}",
        _0
    )]
    ExtensionTag(String),
    #[error(display = "malformed API response: {}", _0)]
    Json(#[error(source)] serde_json::Error),
    #[error(
        display = "malformed API response: structure of group {} in link trail pattern: {:?}",
        LINK_TRAIL_GROUP_INDEX,
        _0
    )]
    LinkTrailInvalidGroup(String),
    #[error(
        display = "malformed API response: no group {} in link trail pattern: {:?}",
        LINK_TRAIL_GROUP_INDEX,
        _0
    )]
    LinkTrailNoGroup(String),
    #[error(display = "malformed API response: no namespace {:?}", _0)]
    NoNamespace(String),
    #[error(display = "malformed API response: no errors or warnings, and no query")]
    NoQuery,
    #[error(display = "malformed API response: {}", _0)]
    PCRE(#[error(source)] pcre::Error),
    #[error(display = "malformed API response: {}", _0)]
    Response(#[error(source)] response::Errors),
}

impl Endpoint {
    pub(crate) fn fetch(&self) -> Result<response::Response, reqwest::Error> {
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

        response.json()
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

impl response::Query {
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

impl convert::TryFrom<response::Response> for response::Query {
    type Error = MalformedError;

    fn try_from(response: response::Response) -> Result<Self, Self::Error> {
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
