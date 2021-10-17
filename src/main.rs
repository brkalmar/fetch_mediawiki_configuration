use serde::Deserialize;
use std::{env, error, fmt, process};

#[derive(Debug, Deserialize)]
struct Response {
    query: Option<Box<serde_json::value::RawValue>>,

    errors: Option<ResponseErrors>,
    warnings: Option<ResponseErrors>,
}

#[derive(Debug, Deserialize)]
struct ResponseErrors(Vec<ResponseError>);

#[derive(Debug, Deserialize)]
struct ResponseError {
    code: String,
    data: Option<serde_json::Value>,
    module: String,
    text: String,
}

#[derive(Debug)]
struct NoQueryError;

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

impl fmt::Display for NoQueryError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "API response contains no errors and no query")
    }
}

impl error::Error for NoQueryError {}

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
    let query = response.query.ok_or(NoQueryError)?;

    todo!()
}
