use thiserror::Error;

/// Interpreter errors, named after the PLRM's standard error names so that
/// behavior can track the spec as coverage grows. When the `errordict`
/// machinery lands in a later stage, these map directly onto it.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum PsError {
    #[error("stackunderflow")]
    StackUnderflow,
    #[error("execstackoverflow")]
    ExecStackOverflow,
    #[error("typecheck")]
    Typecheck,
    #[error("rangecheck")]
    Rangecheck,
    #[error("undefined in {0}")]
    Undefined(String),
    #[error("undefinedresult")]
    UndefinedResult,
    #[error("unmatchedmark")]
    UnmatchedMark,
    #[error("nocurrentpoint")]
    NoCurrentPoint,
    #[error("invalidexit")]
    InvalidExit,
    #[error("dictstackunderflow")]
    DictStackUnderflow,
    #[error("invalidfont")]
    InvalidFont,
    #[error("invalidfileaccess")]
    InvalidFileAccess,
    #[error("undefinedfilename")]
    UndefinedFilename,
    #[error("syntaxerror: {0}")]
    Syntax(String),
    #[error("limitcheck")]
    Limitcheck,
    #[error("ioerror")]
    Io,
    #[error("invalidrestore")]
    InvalidRestore,
    #[error("undefinedresource")]
    UndefinedResource,
}

impl PsError {
    /// The bare PLRM error name, without any payload detail — what a real
    /// interpreter would report as the error's name.
    pub fn name(&self) -> &'static str {
        match self {
            PsError::StackUnderflow => "stackunderflow",
            PsError::ExecStackOverflow => "execstackoverflow",
            PsError::Typecheck => "typecheck",
            PsError::Rangecheck => "rangecheck",
            PsError::Undefined(_) => "undefined",
            PsError::UndefinedResult => "undefinedresult",
            PsError::UnmatchedMark => "unmatchedmark",
            PsError::NoCurrentPoint => "nocurrentpoint",
            PsError::InvalidExit => "invalidexit",
            PsError::DictStackUnderflow => "dictstackunderflow",
            PsError::InvalidFont => "invalidfont",
            PsError::InvalidFileAccess => "invalidfileaccess",
            PsError::UndefinedFilename => "undefinedfilename",
            PsError::Syntax(_) => "syntaxerror",
            PsError::Limitcheck => "limitcheck",
            PsError::Io => "ioerror",
            PsError::InvalidRestore => "invalidrestore",
            PsError::UndefinedResource => "undefinedresource",
        }
    }

    /// The inverse of [`PsError::name`], for rebuilding the error a
    /// `stopped` recorded in `$error` (issue #142).
    ///
    /// `$error` is the authority rather than any Rust-side cache,
    /// because it is a VM dict: a `restore` rolls it back like anything
    /// else, and can leave it naming an *earlier* error than the one
    /// last caught. `command` supplies `undefined`'s payload, which
    /// `$error` keeps in its own `/command` entry. `syntaxerror`'s
    /// detail has nowhere to live in `$error`, so it comes back empty —
    /// callers that still hold the original should prefer it.
    pub fn from_name(name: &str, command: Option<String>) -> Option<PsError> {
        Some(match name {
            "stackunderflow" => PsError::StackUnderflow,
            "execstackoverflow" => PsError::ExecStackOverflow,
            "typecheck" => PsError::Typecheck,
            "rangecheck" => PsError::Rangecheck,
            "undefined" => PsError::Undefined(command.unwrap_or_default()),
            "undefinedresult" => PsError::UndefinedResult,
            "unmatchedmark" => PsError::UnmatchedMark,
            "nocurrentpoint" => PsError::NoCurrentPoint,
            "invalidexit" => PsError::InvalidExit,
            "dictstackunderflow" => PsError::DictStackUnderflow,
            "invalidfont" => PsError::InvalidFont,
            "invalidfileaccess" => PsError::InvalidFileAccess,
            "undefinedfilename" => PsError::UndefinedFilename,
            "syntaxerror" => PsError::Syntax(String::new()),
            "limitcheck" => PsError::Limitcheck,
            "ioerror" => PsError::Io,
            "invalidrestore" => PsError::InvalidRestore,
            "undefinedresource" => PsError::UndefinedResource,
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::PsError;

    /// Every variant must survive the round trip, or a `stop` re-raise
    /// would silently drop an error kind (issue #142).
    #[test]
    fn every_error_name_rebuilds_into_its_own_variant() {
        for e in [
            PsError::StackUnderflow,
            PsError::ExecStackOverflow,
            PsError::Typecheck,
            PsError::Rangecheck,
            PsError::Undefined("thing".to_string()),
            PsError::UndefinedResult,
            PsError::UnmatchedMark,
            PsError::NoCurrentPoint,
            PsError::InvalidExit,
            PsError::DictStackUnderflow,
            PsError::InvalidFont,
            PsError::InvalidFileAccess,
            PsError::UndefinedFilename,
            PsError::Syntax(String::new()),
            PsError::Limitcheck,
            PsError::Io,
            PsError::InvalidRestore,
            PsError::UndefinedResource,
        ] {
            let command = match &e {
                PsError::Undefined(n) => Some(n.clone()),
                _ => None,
            };
            assert_eq!(
                PsError::from_name(e.name(), command).as_ref(),
                Some(&e),
                "{} did not round-trip through its own name",
                e.name()
            );
        }
        assert_eq!(PsError::from_name("notanerror", None), None);
    }
}
