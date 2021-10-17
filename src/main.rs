use std::{env, error, process};

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
Fetch the site configuration of a MediaWiki based wiki, and output rust code for creating a \
configuration for `parse_wiki_text` specific to that wiki.  Write generated code to stdout.  Write \
log messages to stderr.

Maximum log level can be set in env variable `{}` to one of `off`, `error`, `warn`, `info`, \
`debug`, `trace`.",
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

    todo!()
}
