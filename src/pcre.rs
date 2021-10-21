use regex_syntax::{ast, hir};
use std::{error, fmt};

mod private {
    pub(crate) trait Sealed {}
}

#[derive(Debug)]
pub(crate) struct Pattern {
    pub(crate) hir: hir::Hir,
    modifiers: Modifiers,
}

#[derive(Debug, Default)]
pub(crate) struct Modifiers {
    // Passed to parser.
    extended: bool,

    // Passed to translator.
    caseless: bool,
    dotall: bool,
    multiline: bool,
    ungreedy: bool,

    // Ignored: not relevant.
    anchored: bool,
    dollar_endonly: bool,
    extra: bool,
    utf8: bool,

    // Ignored: no effect.
    speedup: bool,

    // Error: not supported.
    info_jchanged: bool,
}

#[derive(Debug)]
pub(crate) enum Error {
    ModifierUnsupported(char),
    Modifiers(char),
    Pattern(String),
    Regex(regex_syntax::Error),
}

pub(crate) trait HirExt: private::Sealed {
    fn find_group_index(&self, index: u32) -> Option<&hir::Group>;
}

impl std::str::FromStr for Pattern {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut delimiter = None;

        for (i, b) in s.as_bytes().iter().enumerate() {
            if b.is_ascii_whitespace() {
                // NOTE: Skip leading whitespace.
            } else if b.is_ascii() && !b.is_ascii_alphanumeric() && *b != b'\\' {
                // Found delimiter.
                delimiter = Some((*b, i));
                break;
            } else {
                // Unexpected character.
                break;
            }
        }

        let (delimiter_end, regex_start) = match delimiter {
            Some((delimiter, index)) => {
                let end = match delimiter {
                    b'(' => b')',
                    b'<' => b'>',
                    b'[' => b']',
                    b'{' => b'}',
                    _ => delimiter,
                };
                (end, index + 1)
            }
            None => {
                return Err(Error::Pattern(s.to_owned()));
            }
        };

        let mut rsplit = s.as_bytes()[regex_start..].rsplitn(2, |b| *b == delimiter_end);
        let modifiers = rsplit.next().unwrap();

        // UNSAFE: Ok because the byte slice is split on an ASCII byte, which is a character so
        // character boundaries are properly aligned.  (Also checked in the debug assertion.)
        debug_assert!(std::str::from_utf8(modifiers).is_ok());
        let modifiers = unsafe { std::str::from_utf8_unchecked(modifiers) };
        let regex = rsplit.next().ok_or_else(|| Error::Pattern(s.to_owned()))?;

        // UNSAFE: See above.
        debug_assert!(std::str::from_utf8(regex).is_ok());
        let regex = unsafe { std::str::from_utf8_unchecked(regex) };

        debug_assert!(rsplit.next().is_none());

        let modifiers: Modifiers = modifiers.parse()?;
        if modifiers.info_jchanged {
            return Err(Error::ModifierUnsupported('J'));
        }

        let mut parser = ast::parse::ParserBuilder::default()
            .ignore_whitespace(modifiers.extended)
            .build();
        let ast = parser
            .parse(regex)
            .map_err(Into::into)
            .map_err(Error::Regex)?;
        let mut translator = hir::translate::TranslatorBuilder::default()
            .case_insensitive(modifiers.caseless)
            .dot_matches_new_line(modifiers.dotall)
            .multi_line(modifiers.multiline)
            .swap_greed(modifiers.ungreedy)
            .build();
        let hir = translator
            .translate(regex, &ast)
            .map_err(Into::into)
            .map_err(Error::Regex)?;

        Ok(Self { hir, modifiers })
    }
}

impl std::str::FromStr for Modifiers {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut modifiers = Self::default();
        for (i, b) in s.bytes().enumerate() {
            match b {
                b'\n' | b'\r' | b' ' => {
                    // NOTE: Skip newline or space.
                }
                b'i' => modifiers.caseless = true,
                b'm' => modifiers.multiline = true,
                b's' => modifiers.dotall = true,
                b'x' => modifiers.extended = true,
                b'A' => modifiers.anchored = true,
                b'D' => modifiers.dollar_endonly = true,
                b'S' => modifiers.speedup = true,
                b'U' => modifiers.ungreedy = true,
                b'X' => modifiers.extra = true,
                b'J' => modifiers.info_jchanged = true,
                b'u' => modifiers.utf8 = true,
                _ => return Err(Error::Modifiers(s[i..].chars().next().unwrap())),
            }
        }
        Ok(modifiers)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use Error::*;

        write!(f, "PHP PCRE ")?;
        match self {
            ModifierUnsupported(c) => write!(f, "unsupported modifier: {:?}", c)?,
            Modifiers(c) => write!(f, "unrecognized modifier: {:?}", c)?,
            Pattern(pattern) => write!(f, "invalid pattern: {:?}", pattern)?,
            Regex(e) => write!(f, "invalid regex: {}", e)?,
        }
        Ok(())
    }
}

impl error::Error for Error {}

impl HirExt for hir::Hir {
    fn find_group_index(&self, index: u32) -> Option<&hir::Group> {
        use hir::GroupKind::*;
        use hir::HirKind::*;
        match self.kind() {
            Concat(hirs) | Alternation(hirs) => {
                hirs.iter().filter_map(|h| h.find_group_index(index)).next()
            }
            Group(group) => {
                let found = match group.kind {
                    CaptureIndex(i) | CaptureName { index: i, name: _ } => i == index,
                    NonCapturing => false,
                };
                if found {
                    Some(group)
                } else {
                    group.hir.find_group_index(index)
                }
            }
            Repetition(repetition) => repetition.hir.find_group_index(index),
            Anchor(..) | Class(..) | Empty | Literal(..) | WordBoundary(..) => None,
        }
    }
}

impl private::Sealed for hir::Hir {}
