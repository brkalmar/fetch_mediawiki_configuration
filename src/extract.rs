use crate::{pcre, siteinfo};
use err_derive::Error;
use pcre::HirExt;
use regex_syntax::hir;
use std::{collections, iter};

#[derive(Debug)]
pub(crate) struct ConfigurationSource {
    pub(crate) category_namespaces: collections::BTreeSet<String>,
    pub(crate) extension_tags: collections::BTreeSet<String>,
    pub(crate) file_namespaces: collections::BTreeSet<String>,
    pub(crate) link_trail: collections::BTreeSet<char>,
    pub(crate) magic_words: collections::BTreeSet<String>,
    pub(crate) protocols: collections::BTreeSet<String>,
    pub(crate) redirect_magic_words: collections::BTreeSet<String>,
}

#[derive(Debug, Error)]
pub(crate) enum Error {
    #[error(display = "{}", _0)]
    LinkTrail(#[error(source)] LinkTrailError),
    #[error(display = "{}", _0)]
    MalformedExtensionTag(#[error(source)] MalformedExtensionTagError),
    #[error(display = "{}", _0)]
    NamespaceNotFound(#[error(source)] NamespaceNotFoundError),
}

#[derive(Debug, Error)]
#[error(display = "namespace not found: {:?}", _0)]
pub(crate) struct NamespaceNotFoundError(String);

#[derive(Debug, Error)]
#[error(display = "malformed extension tag: {:?}", _0)]
pub(crate) struct MalformedExtensionTagError(String);

#[derive(Debug, Error)]
pub(crate) enum LinkTrailError {
    #[error(
        display = "group {} not found in link trail pattern: {:?}",
        index,
        pattern
    )]
    GroupNotFound { pattern: String, index: u32 },
    #[error(
        display = "group {} of invalid structure in link trail pattern: {:?}",
        index,
        pattern
    )]
    GroupInvalid { pattern: String, index: u32 },
    #[error(display = "link trail pattern: {}", _0)]
    PCRE(#[error(source)] pcre::PatternParseError),
}

impl LinkTrailError {
    fn group_not_found(pattern: &str, index: u32) -> Self {
        Self::GroupNotFound {
            pattern: pattern.to_owned(),
            index: index,
        }
    }

    fn group_invalid(pattern: &str, index: u32) -> Self {
        Self::GroupInvalid {
            pattern: pattern.to_owned(),
            index: index,
        }
    }
}

pub(crate) fn configuration_source(
    query: &siteinfo::response::Query,
) -> Result<ConfigurationSource, Error> {
    let category_namespaces = namespaces(&query, "Category")?;
    log::debug!(
        "category namespaces: ({}) {:?}",
        category_namespaces.len(),
        category_namespaces
    );
    let file_namespaces = namespaces(&query, "File")?;
    log::debug!(
        "file namespaces: ({}) {:?}",
        file_namespaces.len(),
        file_namespaces
    );

    let extension_tags = extension_tags(&query)?;
    log::debug!(
        "extension tags: ({}) {:?}",
        extension_tags.len(),
        extension_tags
    );
    let protocols = protocols(&query);
    log::debug!("protocols: ({}) {:?}", protocols.len(), protocols);

    let link_trail = link_trail(&query)?;
    if link_trail.len() <= (1 << 7) {
        log::debug!("link trail: ({}) {:?}", link_trail.len(), link_trail);
    } else {
        log::debug!("link trail: ({})", link_trail.len());
    }

    let magic_words = magic_words(&query);
    log::debug!("magic words: ({}) {:?}", magic_words.len(), magic_words);
    let redirect_magic_words = magic_words_redirect(&query);
    log::debug!(
        "redirect magic words: ({}) {:?}",
        redirect_magic_words.len(),
        redirect_magic_words
    );

    Ok(ConfigurationSource {
        category_namespaces,
        extension_tags,
        file_namespaces,
        link_trail,
        magic_words,
        protocols,
        redirect_magic_words,
    })
}

pub(crate) fn namespaces(
    query: &siteinfo::response::Query,
    canonical: &str,
) -> Result<collections::BTreeSet<String>, NamespaceNotFoundError> {
    let namespace = query
        .namespaces
        .values()
        .find(|ns| ns.canonical.as_ref().map(AsRef::as_ref) == Some(canonical))
        .ok_or_else(|| NamespaceNotFoundError(canonical.to_owned()))?;
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

pub(crate) fn extension_tags(
    query: &siteinfo::response::Query,
) -> Result<collections::BTreeSet<String>, MalformedExtensionTagError> {
    query
        .extensiontags
        .iter()
        .map(|et| {
            et.0.as_str()
                .strip_prefix("<")
                .and_then(|s| s.strip_suffix(">"))
                .map(str::to_lowercase)
                .ok_or(MalformedExtensionTagError(et.0.clone()))
        })
        .collect()
}

pub(crate) fn protocols(query: &siteinfo::response::Query) -> collections::BTreeSet<String> {
    query.protocols.iter().map(|p| p.0.to_lowercase()).collect()
}

pub(crate) fn link_trail(
    query: &siteinfo::response::Query,
) -> Result<collections::BTreeSet<char>, LinkTrailError> {
    use hir::HirKind::*;

    let original = &query.general.linktrail;
    let pattern: pcre::Pattern = original.parse()?;
    log::debug!("pattern = {:?}", pattern);

    const GROUP_INDEX: u32 = 1;
    let group = pattern
        .hir
        .find_group_index(GROUP_INDEX)
        .ok_or_else(|| LinkTrailError::group_not_found(original, GROUP_INDEX))?;
    let repeated = match group.hir.kind() {
        Empty => Ok(None),
        Repetition(repetition) => Ok(Some(&repetition.hir)),
        Alternation(..) | Anchor(..) | Class(..) | Concat(..) | Group(..) | Literal(..)
        | WordBoundary(..) => Err(LinkTrailError::group_invalid(original, GROUP_INDEX)),
    }?;
    log::debug!("repeated = {:?}", repeated.map(|r| pcre::HirDebugAlt(r)));

    let mut characters = Default::default();
    if let Some(repeated) = repeated {
        link_trail_characters(repeated, &mut characters)
            .map_err(|_| LinkTrailError::group_invalid(original, GROUP_INDEX))?;
    }
    Ok(characters)
}

fn link_trail_characters(
    hir: &hir::Hir,
    characters: &mut collections::BTreeSet<char>,
) -> Result<(), ()> {
    use hir::HirKind::*;
    use hir::{Class, Literal};
    match hir.kind() {
        Alternation(hirs) => {
            for hir in hirs {
                link_trail_characters(hir, characters)?;
            }
            Ok(())
        }
        Class(class) => {
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
        Group(group) => link_trail_characters(&group.hir, characters),
        Literal(literal) => {
            let c = match literal {
                Literal::Byte(..) => unreachable!(),
                Literal::Unicode(c) => *c,
            };
            characters.insert(c);
            Ok(())
        }
        Anchor(..) | Concat(..) | Empty | Repetition(..) | WordBoundary(..) => Err(()),
    }
}

pub(crate) fn magic_words(query: &siteinfo::response::Query) -> collections::BTreeSet<String> {
    query
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
        .collect()
}

pub(crate) fn magic_words_redirect(
    query: &siteinfo::response::Query,
) -> collections::BTreeSet<String> {
    const NAME: &str = "redirect";
    const PREFIX: &str = "#";
    query
        .magicwords
        .iter()
        .filter(|mw| mw.name == NAME)
        .flat_map(|mw| mw.aliases.iter())
        .map(|s| s.strip_prefix(PREFIX).unwrap_or(s))
        .chain(iter::once(NAME))
        .map(str::to_lowercase)
        .collect()
}
