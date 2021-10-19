use serde::Deserialize;
use std::{collections, env, error, fmt, process};

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
    NoNamespaceCategory,
    NoQuery,
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
            NoQuery => write!(f, "no errors or warnings, and no query")?,
            NoNamespaceCategory => write!(f, "no namespace `Category`")?,
        }
        Ok(())
    }
}

impl error::Error for MalformedError {}

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

    let matches = clap::App::new(clap::crate_name!())
        .about(clap::crate_description!())
        .long_about(
            format!(
                "\
                Fetch the site configuration of a MediaWiki based wiki, and output rust code for \
                creating a configuration for `parse_wiki_text` specific to that wiki.  Write \
                generated code to stdout.  Write log messages to stderr.\
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

    let domain = clap::value_t!(matches.value_of("domain"), String).unwrap_or_else(|e| e.exit());
    log::info!("wiki domain: {:?}", domain);

    let url = {
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
        url.set_host(Some(&domain))?;
        url
    };
    log::debug!("url = {}", url);

    let user_agent = format!(
        "{}/{} ({})",
        clap::crate_name!(),
        clap::crate_version!(),
        clap::crate_authors!(", ")
    );
    log::debug!("user_agent = {:?}", user_agent);
    let client = reqwest::blocking::Client::builder()
        .user_agent(user_agent)
        .https_only(true)
        .deflate(true)
        .gzip(true)
        .build()?;

    let response = client.get(url).send()?.error_for_status()?;
    log::info!("response status: {}", response.status());
    let log_header = |name| {
        log::debug!(
            "response header: {:?}: {:?}",
            name,
            response.headers().get(&name)
        );
    };
    log_header(reqwest::header::CONNECTION);
    log_header(reqwest::header::CONTENT_ENCODING);
    log_header(reqwest::header::CONTENT_LENGTH);
    log_header(reqwest::header::CONTENT_TYPE);
    log_header(reqwest::header::SERVER);
    log_header(reqwest::header::HeaderName::from_static(
        "mediawiki-api-error",
    ));

    let response: Response = response.json()?;
    if let Some(errors) = response.errors {
        return Err(errors.into());
    }
    if let Some(warnings) = response.warnings {
        return Err(warnings.into());
    }
    let query: Query = serde_json::from_str(response.query.ok_or(MalformedError::NoQuery)?.get())?;
    log::debug!("query extensiontags: {}", query.extensiontags.len());
    log::debug!("query general: {:?}", query.general);
    log::debug!("query magicwords: {}", query.magicwords.len());
    log::debug!("query namespacealiases: {}", query.namespacealiases.len());
    log::debug!("query namespaces: {}", query.namespaces.len());
    log::debug!("query protocols: {}", query.protocols.len());

    let category_namespaces = extract_namespaces(&query, "Category")?;
    log::info!(
        "category namespaces: ({}) {:?}",
        category_namespaces.len(),
        category_namespaces
    );
    let file_namespaces = extract_namespaces(&query, "File")?;
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

    // TODO: link trail

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

    todo!()
}

fn extract_namespaces(
    query: &Query,
    canonical: &str,
) -> Result<collections::BTreeSet<String>, MalformedError> {
    let namespace = query
        .namespaces
        .values()
        .find(|ns| ns.canonical.as_ref().map(AsRef::as_ref) == Some(canonical))
        .ok_or(MalformedError::NoNamespaceCategory)?;
    let aliases = query
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
