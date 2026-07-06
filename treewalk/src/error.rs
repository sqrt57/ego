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

#[derive(Debug, Clone)]
pub struct EgoError {
    pub span: SourceSpan,
    pub message: String,
}

impl EgoError {
    pub fn new(span: SourceSpan, message: String) -> Self {
        Self { span, message }
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
