use crate::pcre;
use err_derive::Error;
use itertools::Itertools;
use std::{convert, env};

pub(crate) mod response;

pub(crate) const LINK_TRAIL_GROUP_INDEX: u32 = 1;

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
