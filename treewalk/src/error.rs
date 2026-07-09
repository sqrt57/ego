use std::fmt;
use std::rc::Rc;

#[derive(Debug, Clone, PartialEq)]
pub struct SourceSpan {
    pub file: Rc<String>,
    pub line: u32,
    pub column: u32,
}

impl SourceSpan {
    pub fn new(file: Rc<String>, line: u32, column: u32) -> Self {
        Self { file, line, column }
    }
}

/// Which built-in exception type (lang-spec.md §10) a runtime `EgoError`
/// should be signalled as when it reaches a point with handler-stack access.
/// `Fatal` errors (lexer/parser failures, and internal-bug guards that a
/// well-formed program can never trigger) never go through `on:Do:` — they
/// stay a plain, uncatchable `EgoSignal::Err` all the way to the top.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    Fatal,
    MessageNotUnderstood,
    BadBlockActivation,
    ZeroDivide,
    PrimitiveError,
}

#[derive(Debug, Clone)]
pub struct EgoError {
    pub span: SourceSpan,
    pub message: String,
    pub kind: ErrorKind,
}

impl EgoError {
    pub fn new(span: SourceSpan, message: String) -> Self {
        Self { span, message, kind: ErrorKind::Fatal }
    }

    pub fn with_kind(span: SourceSpan, message: String, kind: ErrorKind) -> Self {
        Self { span, message, kind }
    }
}

impl fmt::Display for EgoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}:{}: error: {}",
            self.span.file, self.span.line, self.span.column, self.message
        )
    }
}

impl std::error::Error for EgoError {}
