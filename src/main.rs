use err_derive::Error;
use std::{env, io, process};

mod api;
mod extract;
mod generate;

#[derive(Debug)]
struct Args {
    domain: String,
}

#[derive(Debug, Error)]
enum Error {
    #[error(display = "{}", _0)]
    Clap(#[error(no_from, source)] clap::Error),
    #[error(display = "{}", _0)]
    ClapDisplayed(#[error(no_from, source)] clap::Error),
    #[error(display = "I/O error: {}", _0)]
    Io(#[error(source)] io::Error),
    #[error(display = "cannot extract configuration data: {}", _0)]
    Extract(#[error(source)] extract::Error),
    #[error(display = "API endpoint: {}", _0)]
    API(#[error(source)] api::Error),
}

impl Args {
    fn parse(log_var: &str) -> Result<Self, clap::Error> {
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
            .get_matches_safe()?;

        let domain = clap::value_t!(matches.value_of("domain"), _)?;
        Ok(Self { domain })
    }
}

impl From<clap::Error> for Error {
    fn from(e: clap::Error) -> Self {
        use clap::ErrorKind::*;
        match e.kind {
            HelpDisplayed | VersionDisplayed => Self::ClapDisplayed(e),
            _ => Self::Clap(e.into()),
        }
    }
}

fn main() {
    process::exit(match run() {
        Ok(()) => 0,
        Err(Error::ClapDisplayed(e)) => {
            print!("{}", e);
            0
        }
        Err(Error::Clap(e)) => {
            eprint!("{}", e);
            1
        }
        Err(e) => {
            log::error!("{}", e);
            1
        }
    });
}

fn run() -> Result<(), Error> {
    let log_var = log_initialize();
    let args = Args::parse(&log_var)?;

    log::info!("connecting to wiki domain: {:?} ...", args.domain);
    let query = api::fetch_query(&args.domain)?;
    let configuration_source = extract::configuration_source(&query)?;

    log::info!("writing `ConfigurationSource` to stdout ...");
    let out = io::stdout();
    generate::configuration_source(out, &configuration_source)?;

    Ok(())
}

fn log_initialize() -> String {
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
    )
    .unwrap();
    log_var
}
