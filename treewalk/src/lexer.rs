use std::rc::Rc;

use crate::error::{EgoError, SourceSpan};

// ── Token types ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // Literals
    Integer(i64),
    Float(f64),
    Str(String),

    // Identifiers and pseudo-variables
    Ident(String),
    Self_,
    Resend,

    // Selectors
    /// Lowercase-start keyword part, e.g. `"foo:"`.
    Keyword(String),
    /// Capital-start keyword part, e.g. `"At:"`.
    CapKeyword(String),
    /// Sequence of binary chars, e.g. `"+"`, `"<-"`, `"|"`, `"="`.
    Binary(String),

    // Punctuation
    Caret,   // ^
    Dot,     // .
    /// `.` with no whitespace on either side, immediately preceded by an
    /// `Ident`/`Resend` token and immediately followed by the start of a
    /// message — the resend syntax `resend.foo` / `parentName.foo`.
    ResendDot,
    Semi,    // ;
    Colon,   // : (standalone, for :param in arg slots)
    LParen,  // (
    RParen,  // )
    LBrack,  // [
    RBrack,  // ]
    LBrace,  // {
    RBrace,  // }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TokenWithSpan {
    pub token: Token,
    pub span: SourceSpan,
}

// ── Public entry point ─────────────────────────────────────────────────────

pub fn lex(source: &str, file: Rc<String>) -> Result<Vec<TokenWithSpan>, EgoError> {
    Lexer::new(source, file).lex_all()
}

// ── Lexer ──────────────────────────────────────────────────────────────────

struct Lexer {
    chars: Vec<char>,
    pos: usize,
    line: u32,
    col: u32,
    file: Rc<String>,
    /// Position right after the previously emitted token, used to detect
    /// whether the upcoming `.` is glued to it with no whitespace between.
    prev_token_end: usize,
    /// Whether the previously emitted token was an `Ident` or `Resend` —
    /// the only tokens a `ResendTarget` can be.
    prev_token_is_resend_target: bool,
}

impl Lexer {
    fn new(source: &str, file: Rc<String>) -> Self {
        Self {
            chars: source.chars().collect(),
            pos: 0,
            line: 1,
            col: 1,
            file,
            prev_token_end: 0,
            prev_token_is_resend_target: false,
        }
    }

    // ── Character access ───────────────────────────────────────────────────

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<char> {
        self.chars.get(self.pos + offset).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.chars.get(self.pos).copied()?;
        self.pos += 1;
        if ch == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(ch)
    }

    fn span(&self) -> SourceSpan {
        SourceSpan::new(self.file.clone(), self.line, self.col)
    }

    fn err(&self, span: SourceSpan, msg: impl Into<String>) -> EgoError {
        EgoError::new(span, msg.into())
    }

    // ── Whitespace and comments ────────────────────────────────────────────

    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(' ' | '\t' | '\r' | '\n')) {
            self.advance();
        }
    }

    /// Skip a comment body.  The opening `"` has already been consumed.
    fn skip_comment(&mut self, open_span: SourceSpan) -> Result<(), EgoError> {
        loop {
            match self.advance() {
                None => return Err(self.err(open_span, "unterminated comment")),
                Some('"') => {
                    if self.peek() == Some('"') {
                        self.advance(); // `""` inside a comment is a literal `"`
                    } else {
                        return Ok(());
                    }
                }
                Some(_) => {}
            }
        }
    }

    // ── Token lexers ───────────────────────────────────────────────────────

    /// Lex a string body.  The opening `'` has already been consumed.
    fn lex_string(&mut self, open_span: SourceSpan) -> Result<TokenWithSpan, EgoError> {
        let mut s = String::new();
        loop {
            match self.advance() {
                None => return Err(self.err(open_span, "unterminated string literal")),
                Some('\'') => {
                    if self.peek() == Some('\'') {
                        self.advance(); // `''` inside a string is a literal `'`
                        s.push('\'');
                    } else {
                        return Ok(TokenWithSpan { token: Token::Str(s), span: open_span });
                    }
                }
                Some(ch) => s.push(ch),
            }
        }
    }

    /// Lex an integer or float starting at the current position.
    /// `negative` is true when a leading `-` has been seen but not yet consumed.
    fn lex_number(&mut self, negative: bool, span: SourceSpan) -> Result<TokenWithSpan, EgoError> {
        if negative {
            self.advance(); // consume `-`
        }

        // Collect the decimal integer part (digits before `.` or `r`).
        let mut int_digits = String::new();
        while matches!(self.peek(), Some('0'..='9')) {
            int_digits.push(self.advance().unwrap());
        }

        // Based integer: NrDIGITS
        if matches!(self.peek(), Some('r' | 'R')) {
            self.advance(); // consume `r`/`R`
            let base: u32 = int_digits
                .parse()
                .map_err(|_| self.err(span.clone(), "invalid base in integer literal"))?;
            if base < 2 || base > 36 {
                return Err(self.err(span, format!("base {base} out of range (2–36)")));
            }
            let mut value: i64 = 0;
            let mut has_digit = false;
            loop {
                let Some(ch) = self.peek() else { break };
                let Some(d) = digit_value(ch) else { break };
                if d >= base {
                    return Err(self.err(
                        span.clone(),
                        format!("digit '{ch}' out of range for base {base}"),
                    ));
                }
                self.advance();
                has_digit = true;
                value = value
                    .checked_mul(base as i64)
                    .and_then(|v| v.checked_add(d as i64))
                    .ok_or_else(|| self.err(span.clone(), "integer literal overflow"))?;
            }
            if !has_digit {
                return Err(self.err(span, "based integer has no digits after 'r'"));
            }
            if negative {
                value = value.checked_neg().ok_or_else(|| {
                    self.err(span.clone(), "integer literal overflow")
                })?;
            }
            return Ok(TokenWithSpan { token: Token::Integer(value), span });
        }

        // Float: requires `.` followed immediately by a decimal digit.
        if self.peek() == Some('.') && matches!(self.peek_at(1), Some('0'..='9')) {
            self.advance(); // consume `.`
            let mut frac_digits = String::new();
            while matches!(self.peek(), Some('0'..='9')) {
                frac_digits.push(self.advance().unwrap());
            }

            let mut exp = String::new();
            if matches!(self.peek(), Some('e' | 'E')) {
                exp.push(self.advance().unwrap()); // `e` or `E`
                if self.peek() == Some('-') {
                    exp.push(self.advance().unwrap());
                }
                let digits_start = exp.len();
                while matches!(self.peek(), Some('0'..='9')) {
                    exp.push(self.advance().unwrap());
                }
                if exp.len() == digits_start {
                    return Err(self.err(span, "float exponent has no digits"));
                }
            }

            let float_str = format!("{int_digits}.{frac_digits}{exp}");
            let mut value: f64 = float_str
                .parse()
                .map_err(|_| self.err(span.clone(), format!("invalid float literal: {float_str}")))?;
            if negative {
                value = -value;
            }
            return Ok(TokenWithSpan { token: Token::Float(value), span });
        }

        // Plain decimal integer.
        if int_digits.is_empty() {
            return Err(self.err(span, "expected digit"));
        }
        let mut value: i64 = int_digits
            .parse()
            .map_err(|_| self.err(span.clone(), "integer literal out of range"))?;
        if negative {
            value = value.checked_neg().ok_or_else(|| {
                self.err(span.clone(), "integer literal overflow")
            })?;
        }
        Ok(TokenWithSpan { token: Token::Integer(value), span })
    }

    /// Lex a lowercase/underscore-start identifier, keyword part, or pseudo-variable.
    fn lex_ident(&mut self, span: SourceSpan) -> TokenWithSpan {
        let mut name = String::new();
        while matches!(self.peek(), Some(c) if c.is_ascii_alphanumeric() || c == '_') {
            name.push(self.advance().unwrap());
        }
        if self.peek() == Some(':') {
            self.advance();
            name.push(':');
            return TokenWithSpan { token: Token::Keyword(name), span };
        }
        let token = match name.as_str() {
            "self" => Token::Self_,
            "resend" => Token::Resend,
            _ => Token::Ident(name),
        };
        TokenWithSpan { token, span }
    }

    /// Lex a capital-start token: must be a cap keyword part ending with `:`.
    fn lex_cap_keyword(&mut self, span: SourceSpan) -> Result<TokenWithSpan, EgoError> {
        let mut name = String::new();
        while matches!(self.peek(), Some(c) if c.is_ascii_alphanumeric() || c == '_') {
            name.push(self.advance().unwrap());
        }
        if self.peek() == Some(':') {
            self.advance();
            name.push(':');
            return Ok(TokenWithSpan { token: Token::CapKeyword(name), span });
        }
        Err(self.err(
            span,
            format!("uppercase token '{name}' must be a keyword part ending with ':'"),
        ))
    }

    /// Lex a maximal sequence of binary characters.
    fn lex_binary(&mut self, span: SourceSpan) -> TokenWithSpan {
        let mut sel = String::new();
        while matches!(self.peek(), Some(c) if is_binary_char(c)) {
            sel.push(self.advance().unwrap());
        }
        TokenWithSpan { token: Token::Binary(sel), span }
    }

    // ── Main dispatch ──────────────────────────────────────────────────────

    fn next_token(&mut self) -> Result<Option<TokenWithSpan>, EgoError> {
        loop {
            self.skip_whitespace();

            let span = self.span();
            let ch = match self.peek() {
                None => return Ok(None),
                Some(c) => c,
            };

            // Comment — skip and try again
            if ch == '"' {
                self.advance();
                self.skip_comment(span)?;
                continue;
            }

            // String literal
            if ch == '\'' {
                self.advance();
                return Ok(Some(self.lex_string(span)?));
            }

            // Positive integer or float
            if ch.is_ascii_digit() {
                return Ok(Some(self.lex_number(false, span)?));
            }

            // Negative integer or float: `-` immediately followed by a decimal digit.
            if ch == '-' && matches!(self.peek_at(1), Some('0'..='9')) {
                return Ok(Some(self.lex_number(true, span)?));
            }

            // Lowercase or `_`: identifier, keyword part, or pseudo-variable
            if ch.is_ascii_lowercase() || ch == '_' {
                return Ok(Some(self.lex_ident(span)));
            }

            // Uppercase: cap keyword part (must end with `:`)
            if ch.is_ascii_uppercase() {
                return Ok(Some(self.lex_cap_keyword(span)?));
            }

            // Binary selector (greedy sequence of binary chars)
            if is_binary_char(ch) {
                return Ok(Some(self.lex_binary(span)));
            }

            // `.`: distinguish an ordinary statement/slot separator from a
            // resend dot. A resend dot has no whitespace on either side and
            // sits right after an `Ident`/`Resend` token — see the `Dot`
            // handling comment on `Token::ResendDot`.
            if ch == '.' {
                let dot_pos = self.pos;
                self.advance();
                let tight_left =
                    self.prev_token_is_resend_target && dot_pos == self.prev_token_end;
                let tight_right = matches!(self.peek(), Some(c) if !c.is_whitespace());
                let token = if tight_left && tight_right { Token::ResendDot } else { Token::Dot };
                return Ok(Some(TokenWithSpan { token, span }));
            }

            // Single-character punctuation
            self.advance();
            let token = match ch {
                '^' => Token::Caret,
                ';' => Token::Semi,
                ':' => Token::Colon,
                '(' => Token::LParen,
                ')' => Token::RParen,
                '[' => Token::LBrack,
                ']' => Token::RBrack,
                '{' => Token::LBrace,
                '}' => Token::RBrace,
                _ => return Err(self.err(span, format!("unexpected character: {ch:?}"))),
            };
            return Ok(Some(TokenWithSpan { token, span }));
        }
    }

    fn lex_all(mut self) -> Result<Vec<TokenWithSpan>, EgoError> {
        let mut tokens = Vec::new();
        while let Some(tok) = self.next_token()? {
            self.prev_token_end = self.pos;
            self.prev_token_is_resend_target = matches!(tok.token, Token::Ident(_) | Token::Resend);
            tokens.push(tok);
        }
        Ok(tokens)
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────

fn is_binary_char(ch: char) -> bool {
    matches!(
        ch,
        '+' | '-' | '*' | '/' | '~' | '<' | '>' | '=' | '&' | '|' | '@' | '%' | ',' | '?' | '!'
    )
}

/// Digit value for based integer literals.
/// `'0'–'9'` → 0–9; `'a'–'z'` / `'A'–'Z'` → 10–35.
fn digit_value(ch: char) -> Option<u32> {
    match ch {
        '0'..='9' => Some(ch as u32 - '0' as u32),
        'a'..='z' => Some(ch as u32 - 'a' as u32 + 10),
        'A'..='Z' => Some(ch as u32 - 'A' as u32 + 10),
        _ => None,
    }
}
