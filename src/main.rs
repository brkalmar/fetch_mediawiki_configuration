use convert::TryInto;
use io::Write;
use pcre::HirExt;
use regex_syntax::hir;
use serde::Deserialize;
use std::{collections, convert, env, error, fmt, io, iter, process};

mod pcre;

const LINK_TRAIL_GROUP_INDEX: u32 = 1;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct Response {
    query: Option<Box<serde_json::value::RawValue>>,

    errors: Option<ResponseErrors>,
    warnings: Option<ResponseErrors>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct Query {
    extensiontags: Vec<ExtensionTag>,
    general: General,
    magicwords: Vec<MagicWord>,
    namespacealiases: Vec<NamespaceAlias>,
    namespaces: collections::BTreeMap<String, Namespace>,
    protocols: Vec<Protocol>,
}

#[derive(Debug, Deserialize)]
#[serde(transparent)]
struct ExtensionTag(String);

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct General {
    linktrail: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct MagicWord {
    aliases: Vec<String>,
    case_sensitive: Option<bool>,
    name: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct NamespaceAlias {
    id: i64,
    alias: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct Namespace {
    id: i64,
    name: String,
    canonical: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(transparent)]
struct Protocol(String);

#[derive(Debug)]
struct Args {
    domain: String,
}

#[derive(Debug, Deserialize)]
#[serde(transparent)]
struct ResponseErrors(Vec<ResponseError>);

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct ResponseError {
    code: String,
    data: Option<serde_json::Value>,
    module: String,
    text: String,
}

#[derive(Debug)]
enum MalformedError {
    ExtensionTag(String),
    Json(serde_json::Error),
    LinkTrailInvalidGroup(String),
    LinkTrailNoGroup(String),
    NoNamespace(String),
    NoQuery,
    PCRE(pcre::Error),
    Response(ResponseErrors),
}

impl Query {
    fn namespace_all_names(
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

impl Args {
    fn parse(log_var: &str) -> Self {
        let matches = clap::App::new(clap::crate_name!())
            .about(clap::crate_description!())
            .long_about(
                format!(
                    "\
                    Fetch the site configuration of a MediaWiki based wiki, and output rust code \
                    for creating a configuration for `parse_wiki_text` specific to that wiki.  \
                    Write generated code to stdout, as a constant expression of type \
                    `parse_wiki_text::ConfigurationSource`.  Write log messages to stderr.\
                    \n\n\
                    Maximum log level can be set in env variable `{}` to one of `off`, `error`, \
                    `warn`, `info`, `debug`, `trace`.\
                    ",
                    log_var
                )
                .as_ref(),
            )
            .version(clap::crate_version!())
            .arg(
                clap::Arg::with_name("domain")
                    .help("The domain name of the wiki (e.g. `en.wikipedia.org`)")
                    .required(true),
            )
            .get_matches();

        let domain = clap::value_t!(matches.value_of("domain"), _).unwrap_or_else(|e| e.exit());
        Self { domain }
    }
}

impl fmt::Display for ResponseErrors {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for e in &self.0[..(self.0.len() - 1)] {
            write!(f, "{}; ", e)?;
        }
        write!(f, "{}", self.0.last().unwrap())?;
        Ok(())
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

fn main() {
    process::exit(match run() {
        Ok(()) => 0,
        Err(e) => {
            log::error!("{}", e);
            1
        }
    });
}

fn run() -> Result<(), Box<dyn error::Error>> {
    use hir::HirKind;

    let log_var = log_initialize()?;
    let args = Args::parse(&log_var);

    log::info!("connecting to wiki domain: {:?}", args.domain);
    let client = api_client()?;
    let url = api_url(&args.domain)?;
    log::debug!("url = {}", url);
    let response = client.get(url).send()?.error_for_status()?;
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

    let query: Query = response.json::<Response>()?.try_into()?;
    for (name, value) in [
        (
            "extensiontags",
            format_args!("({})", query.extensiontags.len()),
        ),
        ("general", format_args!("{:?}", query.general)),
        ("magicwords", format_args!("({})", query.magicwords.len())),
        (
            "namespacealiases",
            format_args!("({})", query.namespacealiases.len()),
        ),
        ("namespaces", format_args!("({})", query.namespaces.len())),
        ("protocols", format_args!("({})", query.protocols.len())),
    ] {
        log::debug!("query {}: {}", name, value);
    }

    let category_namespaces = query.namespace_all_names("Category")?;
    log::info!(
        "category namespaces: ({}) {:?}",
        category_namespaces.len(),
        category_namespaces
    );
    let file_namespaces = query.namespace_all_names("File")?;
    log::info!(
        "file namespaces: ({}) {:?}",
        file_namespaces.len(),
        file_namespaces
    );

    let extension_tags: collections::BTreeSet<_> = query
        .extensiontags
        .iter()
        .map(|et| {
            et.0.as_str()
                .strip_prefix("<")
                .and_then(|s| s.strip_suffix(">"))
                .map(str::to_lowercase)
                .ok_or(MalformedError::ExtensionTag(et.0.clone()))
        })
        .collect::<Result<_, _>>()?;
    log::info!(
        "extension tags: ({}) {:?}",
        extension_tags.len(),
        extension_tags
    );

    let protocols: collections::BTreeSet<_> =
        query.protocols.iter().map(|p| p.0.to_lowercase()).collect();
    log::info!("protocols: ({}) {:?}", protocols.len(), protocols);

    let pattern: pcre::Pattern = query
        .general
        .linktrail
        .parse()
        .map_err(MalformedError::PCRE)?;
    log::debug!("pattern = {:?}", pattern);
    let group = pattern
        .hir
        .find_group_index(LINK_TRAIL_GROUP_INDEX)
        .ok_or_else(|| MalformedError::LinkTrailNoGroup(query.general.linktrail.to_owned()))?;
    let repeated = match group.hir.kind() {
        HirKind::Empty => Ok(None),
        HirKind::Repetition(repetition) => Ok(Some(&repetition.hir)),
        HirKind::Alternation(..)
        | HirKind::Anchor(..)
        | HirKind::Class(..)
        | HirKind::Concat(..)
        | HirKind::Group(..)
        | HirKind::Literal(..)
        | HirKind::WordBoundary(..) => Err(MalformedError::LinkTrailInvalidGroup(
            query.general.linktrail.to_owned(),
        )),
    }?;
    log::debug!("repeated = {:?}", repeated.map(|r| pcre::HirDebugAlt(r)));
    let characters = match repeated {
        None => Default::default(),
        Some(repeated) => {
            let mut characters = Default::default();
            extract_link_trail_characters(repeated, &mut characters).map_err(|_| {
                MalformedError::LinkTrailInvalidGroup(query.general.linktrail.clone())
            })?;
            characters
        }
    };
    log::debug!("characters = {:?}", characters);
    let link_trail: String = characters.iter().collect();
    if characters.len() <= (1 << 9) {
        log::info!("link trail: ({}) {:?}", characters.len(), link_trail);
    } else {
        log::info!("link trail: ({})", characters.len());
    }

    let magic_words: collections::BTreeSet<_> = query
        .magicwords
        .iter()
        .flat_map(|mw| {
            mw.aliases
                .iter()
                .map(AsRef::as_ref)
                .chain(iter::once(mw.name.as_str()))
        })
        .filter_map(|s| s.strip_prefix("__").and_then(|s| s.strip_suffix("__")))
        .map(str::to_lowercase)
        .collect();
    log::info!("magic words: ({}) {:?}", magic_words.len(), magic_words);

    const REDIRECT_NAME: &str = "redirect";
    let redirect_magic_words: collections::BTreeSet<_> = query
        .magicwords
        .iter()
        .filter(|mw| mw.name == REDIRECT_NAME)
        .flat_map(|mw| mw.aliases.iter())
        .map(|s| s.strip_prefix("#").unwrap_or(s))
        .chain(iter::once(REDIRECT_NAME))
        .map(str::to_lowercase)
        .collect();
    log::info!(
        "redirect magic words: ({}) {:?}",
        redirect_magic_words.len(),
        redirect_magic_words
    );

    let tokens = quote::quote! {
        ::parse_wiki_text::ConfigurationSource {
            category_namespaces: &[ #( #category_namespaces ),* ],
            extension_tags: &[ #( #extension_tags ),* ],
            file_namespaces: &[ #( #file_namespaces ),* ],
            link_trail: #link_trail ,
            magic_words: &[ #( #magic_words ),* ],
            protocols: &[ #( #protocols ),* ],
            redirect_magic_words: &[ #( #redirect_magic_words ),* ],
        }
    };
    let mut out = io::stdout();
    write!(out, "{}", tokens)?;

    Ok(())
}

fn extract_link_trail_characters(
    hir: &hir::Hir,
    characters: &mut collections::BTreeSet<char>,
) -> Result<(), ()> {
    use hir::{Class, HirKind, Literal};
    match hir.kind() {
        HirKind::Alternation(hirs) => {
            for hir in hirs {
                extract_link_trail_characters(hir, characters)?;
            }
            Ok(())
        }
        HirKind::Class(class) => {
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
        HirKind::Group(group) => extract_link_trail_characters(&group.hir, characters),
        HirKind::Literal(literal) => {
            let c = match literal {
                Literal::Byte(..) => unreachable!(),
                Literal::Unicode(c) => *c,
            };
            characters.insert(c);
            Ok(())
        }
        HirKind::Anchor(..)
        | HirKind::Concat(..)
        | HirKind::Empty
        | HirKind::Repetition(..)
        | HirKind::WordBoundary(..) => Err(()),
    }
}

fn api_client() -> Result<reqwest::blocking::Client, reqwest::Error> {
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

fn api_url(domain: &str) -> Result<reqwest::Url, url::ParseError> {
    const CATEGORIES: &[&str] = &[
        "extensiontags",
        "general",
        "magicwords",
        "namespacealiases",
        "namespaces",
        "protocols",
    ];
    let mut url = reqwest::Url::parse_with_params(
        "https://example.org/w/api.php",
        [
            ("action", "query"),
            ("meta", "siteinfo"),
            ("siprop", &CATEGORIES.join("|")),
            ("format", "json"),
            ("formatversion", "2"),
            ("errorformat", "plaintext"),
        ],
    )
    .unwrap();
    url.set_host(Some(domain))?;
    Ok(url)
}

fn log_initialize() -> Result<String, log::SetLoggerError> {
    let log_var = format!("{}_LOG", clap::crate_name!().to_uppercase());
    simplelog::TermLogger::init(
        env::var(&log_var)
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(simplelog::LevelFilter::Info),
        simplelog::ConfigBuilder::default()
            .set_level_padding(simplelog::LevelPadding::Left)
            .set_thread_level(simplelog::LevelFilter::Trace)
            .set_thread_mode(simplelog::ThreadLogMode::Both)
            .build(),
        simplelog::TerminalMode::Stderr,
        simplelog::ColorChoice::Auto,
    )?;
    Ok(log_var)
}
