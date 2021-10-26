use err_derive::Error;
use std::{env, io, process};

mod api;
mod extract;
mod generate;

#[derive(Debug)]
struct Args {
    domain: String,
    log_level: log::LevelFilter,
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
    Api(#[error(source)] api::Error),
}

impl Args {
    fn parse() -> Result<Self, clap::Error> {
        use log::LevelFilter::*;

        let log_levels: Vec<_> = [Off, Error, Warn, Info, Debug, Trace]
            .iter()
            .map(|l| l.as_str())
            .collect();

        let matches = clap::App::new(clap::crate_name!())
            .about(clap::crate_description!())
            .long_about(
                "\
                Fetch the site configuration of a MediaWiki based wiki, and output rust code for \
                creating a configuration for `parse_wiki_text` specific to that wiki.  Write \
                generated code to stdout, as a constant expression of type \
                `parse_wiki_text::ConfigurationSource`.  Write log messages to stderr.\
                ",
            )
            .version(clap::crate_version!())
            .arg(
                clap::Arg::with_name("domain")
                    .help("The domain name of the wiki (e.g. `en.wikipedia.org`)")
                    .required(true),
            )
            .arg(
                clap::Arg::with_name("log-level")
                    .long("log-level")
                    .help("Maximum log level")
                    .case_insensitive(true)
                    .default_value(Info.as_str())
                    .possible_values(&log_levels),
            )
            .get_matches_safe()?;

        let domain = clap::value_t!(matches.value_of("domain"), _)?;
        let log_level = clap::value_t!(matches.value_of("log-level"), _)?;
        Ok(Self { domain, log_level })
    }
}

impl From<clap::Error> for Error {
    fn from(e: clap::Error) -> Self {
        use clap::ErrorKind::*;
        match e.kind {
            HelpDisplayed | VersionDisplayed => Self::ClapDisplayed(e),
            _ => Self::Clap(e),
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
            2
        }
        Err(e) => {
            log::error!("{}", e);
            1
        }
    });
}

fn run() -> Result<(), Error> {
    let args = Args::parse()?;
    log_initialize(args.log_level);

    log::info!("connect to API at wiki domain: {:?} ...", args.domain);
    let query = api::fetch_query(&args.domain)?;
    log::info!("extract configuration data from response ...");
    let configuration_source = extract::configuration_source(&query)?;

    log::info!("write generated code to stdout ...");
    let out = io::stdout();
    generate::configuration_source(out, &configuration_source)?;

    Ok(())
}

fn log_initialize(level: log::LevelFilter) {
    simplelog::TermLogger::init(
        level,
        simplelog::ConfigBuilder::default()
            .set_level_padding(simplelog::LevelPadding::Left)
            .set_thread_level(log::LevelFilter::Trace)
            .set_thread_mode(simplelog::ThreadLogMode::Both)
            .build(),
        simplelog::TerminalMode::Stderr,
        simplelog::ColorChoice::Auto,
    )
    .unwrap();
}
