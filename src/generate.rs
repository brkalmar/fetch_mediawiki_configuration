use crate::extract;
use std::io;

pub fn configuration_source(
    mut out: impl io::Write,
    configuration_source: &extract::ConfigurationSource,
) -> Result<(), io::Error> {
    let extract::ConfigurationSource {
        category_namespaces,
        extension_tags,
        file_namespaces,
        link_trail,
        magic_words,
        protocols,
        redirect_magic_words,
    } = configuration_source;
    let link_trail: String = link_trail.into_iter().collect();

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
    write!(out, "{}", tokens)?;

    Ok(())
}
