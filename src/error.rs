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
}
