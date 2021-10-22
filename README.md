# Fetch MediaWiki site configuration

Helper script to properly configure [`parse_wiki_text`](https://docs.rs/parse_wiki_text) for different wikis.

Originally at <https://github.com/portstrom/fetch_mediawiki_configuration> ([Internet Archive snapshot](https://web.archive.org/web/20200907151105/https://github.com/portstrom/fetch_mediawiki_configuration)), the repo is now deleted, along with all the user's related wiki repos.
The libraries are preserved in various github forks and on [docs.rs](https://docs.rs).
However, no public copies of `fetch_mediawiki_configuration` are available.

This project is a recreation of the functionality of the original script.
Inferences and assumptions made are documented under [Implementation notes](#implementation-notes).

## Usage

Use `cargo run --` followed by the arguments to run the script.
Pass argument `--help` for more information:
```shell
cargo run -- --help
```

## Implementation notes

All information needed for the [`ConfigurationSource`](https://docs.rs/parse_wiki_text/latest/parse_wiki_text/struct.ConfigurationSource.html) is fetched from the [MediaWiki Action API](https://www.mediawiki.org/wiki/API:Main_page) instance at the given domain.
We use the [query siteinfo metadata endpoint](https://www.mediawiki.org/w/api.php?action=help&modules=query%2Bsiteinfo), with `siprop` set to the categories we need.
See <https://www.mediawiki.org/wiki/API:Siteinfo> for more detailed documentation of the response.

### Normalization

`parse_wiki_text` stores most configuration values using a trie, which implicitly inserts new values with all possible case-folded variants.
For `extension_tags`, a case-sensitive data structure is used, but ASCII characters in the wiki text are converted to lowercase before comparisons.
As extension tags are expected to be ASCII-only, this leaves us with effectively case-insensitive comparison.
For `link_trail`, a [`HashSet<char>`](https://doc.rust-lang.org/std/collections/struct.HashSet.html) is used and the characters in it are compared to the wiki text directly.

In other words, all fields of `ConfigurationSource` are case-insensitive, except for `link_trail`.
This implementation normalizes all configuration values, apart from link-trail characters, to lowercase.
In addition, duplicates are removed and values are sorted in ascending order in each field; although not strictly necessary for correctness, it removes clutter and improves reproducibility.

### Namespaces

`category_namespaces` and `file_namespaces` are extracted from `siprop=namespaces` and `siprop=namespacealiases`.

These fields must contain the primary and canoncial namespace names in addition to the aliases, despite the misleading names and lacking documentation.

### Extension tags and protocols

`extension_tags` is straightforwardly extracted from `siprop=extensiontags`.

`protocols` is straightforwardly extracted from `siprop=protocols`.

### Link trail

`link_trail` is extracted from `siprop=general` under key `linktrail`.

This is a [PHP PCRE](https://www.php.net/manual/en/book.pcre.php) pattern containing two groups: (1) the trailing section to be a part of the link it follows, (2) everything after.

We do some simple parsing of the pattern using the [regex-syntax](https://crates.io/crates/regex-syntax) crate.
The [modifiers](https://www.php.net/manual/en/reference.pcre.pattern.modifiers.php) are also parsed and used where applicable.
If group 1 is empty, the link trail has no characters.
If it is a repetiton structure, the repeated part is extracted recursively, only allowing constructs that yield single-character sequences.
Otherwise, the regex is considered invalid.

### Limitations

This approach only accepts regexes with a specific structure, and does not take into account differences between PHP PCREs and rust regexes.
However, the patterns are not expected to be diverse enough to cause problems with respect to structure, nor complex enough for the syntax differences to surface.
These differences are for the most part minor or edge cases, since both syntaxes derive directly from Perl regexes.

A more serious limitation is that `link_trail` cannot store anything more complicated than a simple set of characters.
If the repeated part in the regex contains concatenations, lookaheads, or similar, it cannot be represented in the field, and a fatal error results.
This does affect a few actual wiki instances (try e.g. [`ca.wiktionary.org`](https://ca.wiktionary.org) or [`se.wikipedia.org`](https://se.wikipedia.org)), but as it is a limitation of `parse_wiki_text` there is currently no way to solve it.

### Magic words

`magic_words` is extracted from `siprop=magicwords`.

All magic words and their aliases are searched.
We only accept those both prefixed and suffixed by `__`, and remove the prefix and suffix.

#### Redirect

`redirect_magic_words` is extracted from the same category.

The aliases of the magic word with name `redirect` are collected.
Since the `parse_wiki_text` parser performs a lookup among these only after it has already found the starting `#`, we remove any starting `#`.
In addition, `redirect` itself must also be included.

## License

This project is licensed under the [MIT license](LICENSE).
