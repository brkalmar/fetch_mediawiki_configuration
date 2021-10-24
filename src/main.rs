use convert::TryInto;
use io::Write;
use std::{convert, env, error, io, process};

mod extract;
mod pcre;
mod siteinfo;

#[derive(Debug)]
struct Args {
    domain: String,
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
    let log_var = log_initialize()?;
    let args = Args::parse(&log_var);

    log::info!("connecting to wiki domain: {:?}", args.domain);
    let endpoint = siteinfo::Endpoint::new(&args.domain)?;
    let query: siteinfo::response::Query = endpoint.fetch()?.try_into()?;

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

    let category_namespaces = extract::namespaces(&query, "Category")?;
    log::info!(
        "category namespaces: ({}) {:?}",
        category_namespaces.len(),
        category_namespaces
    );
    let file_namespaces = extract::namespaces(&query, "File")?;
    log::info!(
        "file namespaces: ({}) {:?}",
        file_namespaces.len(),
        file_namespaces
    );

    let extension_tags = extract::extension_tags(&query)?;
    log::info!(
        "extension tags: ({}) {:?}",
        extension_tags.len(),
        extension_tags
    );
    let protocols = extract::protocols(&query);
    log::info!("protocols: ({}) {:?}", protocols.len(), protocols);

    let link_trail = extract::link_trail(&query)?;
    if link_trail.len() <= (1 << 9) {
        log::info!("link trail: ({}) {:?}", link_trail.len(), link_trail);
    } else {
        log::info!("link trail: ({})", link_trail.len());
    }
    let link_trail: String = link_trail.into_iter().collect();

    let magic_words = extract::magic_words(&query);
    log::info!("magic words: ({}) {:?}", magic_words.len(), magic_words);
    let redirect_magic_words = extract::magic_words_redirect(&query);
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
