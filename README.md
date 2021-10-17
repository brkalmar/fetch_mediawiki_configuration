# Fetch MediaWiki site configuration

Helper script to properly configure [`parse_wiki_text`](https://docs.rs/parse_wiki_text) for different wikis.

Originally at <https://github.com/portstrom/fetch_mediawiki_configuration> ([Internet Archive snapshot](https://web.archive.org/web/20200907151105/https://github.com/portstrom/fetch_mediawiki_configuration)), the repo is now deleted, along with all the user's related wiki repos.
The libraries are preserved in various github forks and on [docs.rs](https://docs.rs).
However, no public copies of `fetch_mediawiki_configuration` are available.

This project is a recreation of the functionality of the original script.
Inferences and assumptions made are documented under [Implementation notes](#implementation-notes).

## Usage

To run the script:
```shell
cargo run -- <domain>
```

Fetches the site configuration of a MediaWiki based wiki, and outputs rust code for creating a configuration for [`parse_wiki_text`](https://docs.rs/parse_wiki_text) specific to that wiki.
The domain name of the wiki (e.g. `en.wikipedia.org`) is taken as command line argument `<domain>`.
The generated code is written to stdout.

## Implementation notes

TODO

## License

This project is licensed under the [MIT license](LICENSE).
