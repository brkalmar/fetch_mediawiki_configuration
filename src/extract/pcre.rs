use err_derive::Error;
use regex_syntax::{ast, hir};
use std::fmt;

mod private {
    pub trait Sealed {}
}

pub struct Pattern {
    pub hir: hir::Hir,
    modifiers: Modifiers,
}

#[derive(Debug, Default)]
struct Modifiers {
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

pub struct HirDebugAlt<'h>(pub &'h hir::Hir);

#[derive(Debug, Error)]
#[error(display = "{}: {:?}", kind, pattern)]
pub struct PatternParseError {
    pub pattern: String,
    pub kind: PatternParseErrorKind,
}

#[derive(Debug)]
pub enum PatternParseErrorKind {
    ModifierUnsupported(char),
    Modifiers(ModifiersParseError),
    Pattern,
    Regex(regex_syntax::Error),
}

#[derive(Debug, Error)]
#[error(display = "unrecognized PHP PCRE modifier: {:?}", _0)]
pub struct ModifiersParseError(char);

pub trait HirExt: private::Sealed {
    fn find_group_index(&self, index: u32) -> Option<&hir::Group>;
}

impl fmt::Debug for Pattern {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Pattern")
            .field("hir", &HirDebugAlt(&self.hir))
            .field("modifiers", &self.modifiers)
            .finish()
    }
}

impl std::str::FromStr for Pattern {
    type Err = PatternParseError;

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
                return Err(PatternParseError::pattern(s));
            }
        };

        let mut rsplit = s.as_bytes()[regex_start..].rsplitn(2, |b| *b == delimiter_end);
        let modifiers = rsplit.next().unwrap();

        // UNSAFE: Ok because the byte slice is split on an ASCII byte, which is a character so
        // character boundaries are properly aligned.  (Also checked in the debug assertion.)
        debug_assert!(std::str::from_utf8(modifiers).is_ok());
        let modifiers = unsafe { std::str::from_utf8_unchecked(modifiers) };
        let regex = rsplit.next().ok_or_else(|| PatternParseError::pattern(s))?;

        // UNSAFE: See above.
        debug_assert!(std::str::from_utf8(regex).is_ok());
        let regex = unsafe { std::str::from_utf8_unchecked(regex) };

        debug_assert!(rsplit.next().is_none());

        let modifiers: Modifiers = modifiers
            .parse()
            .map_err(|e| PatternParseError::modifiers(s, e))?;
        if modifiers.info_jchanged {
            return Err(PatternParseError::modifier_unsupported(s, 'J'));
        }

        let mut parser = ast::parse::ParserBuilder::default()
            .ignore_whitespace(modifiers.extended)
            .build();
        let ast = parser
            .parse(regex)
            .map_err(|e| PatternParseError::regex(s, e.into()))?;
        let mut translator = hir::translate::TranslatorBuilder::default()
            .case_insensitive(modifiers.caseless)
            .dot_matches_new_line(modifiers.dotall)
            .multi_line(modifiers.multiline)
            .swap_greed(modifiers.ungreedy)
            .build();
        let hir = translator
            .translate(regex, &ast)
            .map_err(|e| PatternParseError::regex(s, e.into()))?;

        Ok(Self { hir, modifiers })
    }
}

impl std::str::FromStr for Modifiers {
    type Err = ModifiersParseError;

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
                _ => return Err(ModifiersParseError(s[i..].chars().next().unwrap())),
            }
        }
        Ok(modifiers)
    }
}

impl<'h> fmt::Debug for HirDebugAlt<'h> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Hir({})", self.0)
    }
}

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

impl PatternParseError {
    fn modifier_unsupported(pattern: &str, c: char) -> Self {
        Self {
            pattern: pattern.to_owned(),
            kind: PatternParseErrorKind::ModifierUnsupported(c),
        }
    }

    fn modifiers(pattern: &str, e: ModifiersParseError) -> Self {
        Self {
            pattern: pattern.to_owned(),
            kind: PatternParseErrorKind::Modifiers(e),
        }
    }

    fn pattern(pattern: &str) -> Self {
        Self {
            pattern: pattern.to_owned(),
            kind: PatternParseErrorKind::Pattern,
        }
    }

    fn regex(pattern: &str, e: regex_syntax::Error) -> Self {
        Self {
            pattern: pattern.to_owned(),
            kind: PatternParseErrorKind::Regex(e),
        }
    }
}

impl fmt::Display for PatternParseErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use PatternParseErrorKind::*;
        match self {
            ModifierUnsupported(c) => write!(f, "unsupported PHP PCRE modifier: {:?}", c),
            Modifiers(e) => write!(f, "{}", e),
            Pattern => write!(f, "invalid PHP PCRE pattern"),
            Regex(e) => write!(f, "invalid PHP PCRE regex: {}", e),
        }
    }
}
