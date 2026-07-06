use rstest::rstest;
use std::rc::Rc;
use treewalk::lexer::{lex, Token};

fn file() -> Rc<String> {
    Rc::new("<test>".to_string())
}

fn tokens(source: &str) -> Vec<Token> {
    lex(source, file())
        .expect("unexpected lex error")
        .into_iter()
        .map(|t| t.token)
        .collect()
}

fn lex_err(source: &str) -> String {
    lex(source, file())
        .expect_err("expected a lex error but got Ok")
        .message
}

// ── Integers ───────────────────────────────────────────────────────────────

#[rstest]
#[case("0",   Token::Integer(0))]
#[case("42",  Token::Integer(42))]
#[case("123", Token::Integer(123))]
fn integer_positive(#[case] src: &str, #[case] tok: Token) {
    assert_eq!(tokens(src), vec![tok]);
}

#[rstest]
#[case("-1",   Token::Integer(-1))]
#[case("-42",  Token::Integer(-42))]
#[case("-0",   Token::Integer(0))]
fn integer_negative(#[case] src: &str, #[case] tok: Token) {
    assert_eq!(tokens(src), vec![tok]);
}

#[rstest]
#[case("16rFF",   Token::Integer(255))]
#[case("16rff",   Token::Integer(255))]
#[case("16r0",    Token::Integer(0))]
#[case("8r17",    Token::Integer(15))]
#[case("2r1010",  Token::Integer(10))]
#[case("2r0",     Token::Integer(0))]
#[case("10r42",   Token::Integer(42))]
#[case("36rZ",    Token::Integer(35))]
#[case("36rz",    Token::Integer(35))]
fn integer_based(#[case] src: &str, #[case] tok: Token) {
    assert_eq!(tokens(src), vec![tok]);
}

#[rstest]
#[case("-16rFF", Token::Integer(-255))]
#[case("-2r1010", Token::Integer(-10))]
fn integer_based_negative(#[case] src: &str, #[case] tok: Token) {
    assert_eq!(tokens(src), vec![tok]);
}

#[test]
fn integer_based_no_digits_error() {
    assert!(lex_err("16r").contains("no digits"));
}

#[test]
fn integer_based_digit_out_of_range_error() {
    assert!(lex_err("2r2").contains("out of range"));
}

#[test]
fn integer_based_base_out_of_range_error() {
    assert!(lex_err("37r0").contains("out of range"));
    assert!(lex_err("1r0").contains("out of range"));
}

// ── Floats ─────────────────────────────────────────────────────────────────

#[rstest]
#[case("3.14",    Token::Float(3.14))]
#[case("1.0",     Token::Float(1.0))]
#[case("0.5",     Token::Float(0.5))]
#[case("2.0e3",   Token::Float(2000.0))]
#[case("2.0E3",   Token::Float(2000.0))]
#[case("2.0e-3",  Token::Float(0.002))]
#[case("2.0E-3",  Token::Float(0.002))]
fn float_positive(#[case] src: &str, #[case] tok: Token) {
    assert_eq!(tokens(src), vec![tok]);
}

#[rstest]
#[case("-3.14",   Token::Float(-3.14))]
#[case("-1.5e10", Token::Float(-1.5e10))]
#[case("-2.0E-3", Token::Float(-0.002))]
fn float_negative(#[case] src: &str, #[case] tok: Token) {
    assert_eq!(tokens(src), vec![tok]);
}

#[test]
fn float_dot_not_followed_by_digit_is_integer_then_dot() {
    // `3.` with no digit after `.` — lexed as integer `3` then dot
    assert_eq!(tokens("3."), vec![Token::Integer(3), Token::Dot]);
}

#[test]
fn float_exponent_no_digits_error() {
    assert!(lex_err("1.0e").contains("no digits"));
    assert!(lex_err("1.0e-").contains("no digits"));
}

// ── Strings ────────────────────────────────────────────────────────────────

#[rstest]
#[case("'hello'",   "hello")]
#[case("''",        "")]
#[case("''''",      "'")]
#[case("'it''s'",   "it's")]
#[case("'a''b''c'", "a'b'c")]
fn string_literals(#[case] src: &str, #[case] expected: &str) {
    assert_eq!(tokens(src), vec![Token::Str(expected.to_string())]);
}

#[test]
fn string_with_newline() {
    assert_eq!(tokens("'a\nb'"), vec![Token::Str("a\nb".to_string())]);
}

#[test]
fn string_unterminated_error() {
    assert!(lex_err("'hello").contains("unterminated"));
}

// ── Identifiers ────────────────────────────────────────────────────────────

#[rstest]
#[case("foo",      Token::Ident("foo".into()))]
#[case("_bar",     Token::Ident("_bar".into()))]
#[case("_",        Token::Ident("_".into()))]
#[case("x42",      Token::Ident("x42".into()))]
#[case("camelCase",Token::Ident("camelCase".into()))]
#[case("nil",      Token::Ident("nil".into()))]
#[case("true",     Token::Ident("true".into()))]
#[case("false",    Token::Ident("false".into()))]
fn identifiers(#[case] src: &str, #[case] tok: Token) {
    assert_eq!(tokens(src), vec![tok]);
}

// ── Pseudo-variables ───────────────────────────────────────────────────────

#[test]
fn pseudo_self() {
    assert_eq!(tokens("self"), vec![Token::Self_]);
}

#[test]
fn pseudo_resend() {
    assert_eq!(tokens("resend"), vec![Token::Resend]);
}

// ── Keyword selectors ──────────────────────────────────────────────────────

#[rstest]
#[case("foo:",    Token::Keyword("foo:".into()))]
#[case("at:",     Token::Keyword("at:".into()))]
#[case("_key:",   Token::Keyword("_key:".into()))]
fn keyword_parts(#[case] src: &str, #[case] tok: Token) {
    assert_eq!(tokens(src), vec![tok]);
}

#[test]
fn two_keyword_parts() {
    assert_eq!(
        tokens("at:put:"),
        vec![Token::Keyword("at:".into()), Token::Keyword("put:".into())]
    );
}

#[rstest]
#[case("At:",      Token::CapKeyword("At:".into()))]
#[case("IfTrue:",  Token::CapKeyword("IfTrue:".into()))]
#[case("Value42:", Token::CapKeyword("Value42:".into()))]
fn cap_keyword_parts(#[case] src: &str, #[case] tok: Token) {
    assert_eq!(tokens(src), vec![tok]);
}

#[test]
fn two_cap_keyword_parts() {
    assert_eq!(
        tokens("Value:With:"),
        vec![Token::CapKeyword("Value:".into()), Token::CapKeyword("With:".into())]
    );
}

#[test]
fn uppercase_without_colon_error() {
    assert!(lex_err("Foo").contains("keyword part"));
}

// ── Binary selectors ───────────────────────────────────────────────────────

#[rstest]
#[case("+",  Token::Binary("+".into()))]
#[case("*",  Token::Binary("*".into()))]
#[case("/",  Token::Binary("/".into()))]
#[case("~",  Token::Binary("~".into()))]
#[case("<",  Token::Binary("<".into()))]
#[case(">",  Token::Binary(">".into()))]
#[case("=",  Token::Binary("=".into()))]
#[case("&",  Token::Binary("&".into()))]
#[case("|",  Token::Binary("|".into()))]
#[case("@",  Token::Binary("@".into()))]
#[case("%",  Token::Binary("%".into()))]
#[case(",",  Token::Binary(",".into()))]
#[case("?",  Token::Binary("?".into()))]
#[case("!",  Token::Binary("!".into()))]
fn binary_single_char(#[case] src: &str, #[case] tok: Token) {
    assert_eq!(tokens(src), vec![tok]);
}

#[rstest]
#[case("<=",  Token::Binary("<=".into()))]
#[case(">=",  Token::Binary(">=".into()))]
#[case("~=",  Token::Binary("~=".into()))]
#[case("<-",  Token::Binary("<-".into()))]
#[case(">>",  Token::Binary(">>".into()))]
#[case("||",  Token::Binary("||".into()))]
fn binary_multi_char(#[case] src: &str, #[case] tok: Token) {
    assert_eq!(tokens(src), vec![tok]);
}

// ── Punctuation ────────────────────────────────────────────────────────────

#[rstest]
#[case("^", Token::Caret)]
#[case(".", Token::Dot)]
#[case(";", Token::Semi)]
#[case(":", Token::Colon)]
#[case("(", Token::LParen)]
#[case(")", Token::RParen)]
#[case("[", Token::LBrack)]
#[case("]", Token::RBrack)]
#[case("{", Token::LBrace)]
#[case("}", Token::RBrace)]
fn punctuation(#[case] src: &str, #[case] tok: Token) {
    assert_eq!(tokens(src), vec![tok]);
}

// ── Comments ───────────────────────────────────────────────────────────────

#[test]
fn comment_only() {
    assert_eq!(tokens(r#""this is a comment""#), vec![]);
}

#[test]
fn comment_with_escaped_quote() {
    // `""` inside a comment is a literal `"`, not the closing delimiter
    assert_eq!(tokens(r#""say ""hello"" world""#), vec![]);
}

#[test]
fn comment_between_tokens() {
    assert_eq!(
        tokens(r#"42 "a comment" 43"#),
        vec![Token::Integer(42), Token::Integer(43)]
    );
}

#[test]
fn comment_unterminated_error() {
    assert!(lex_err(r#""unterminated"#).contains("unterminated"));
}

// ── Whitespace ─────────────────────────────────────────────────────────────

#[test]
fn whitespace_is_skipped() {
    assert_eq!(tokens("  42  "), vec![Token::Integer(42)]);
    assert_eq!(tokens("\t42\n"), vec![Token::Integer(42)]);
    assert_eq!(tokens("42\r\n43"), vec![Token::Integer(42), Token::Integer(43)]);
}

// ── Negative literal vs binary `-` (after_expr state) ─────────────────────

#[test]
fn minus_after_ident_is_binary() {
    // `foo -3` → ident, binary `-`, integer `3`
    assert_eq!(
        tokens("foo -3"),
        vec![Token::Ident("foo".into()), Token::Binary("-".into()), Token::Integer(3)]
    );
}

#[test]
fn minus_after_integer_is_binary() {
    assert_eq!(
        tokens("3 - 4"),
        vec![Token::Integer(3), Token::Binary("-".into()), Token::Integer(4)]
    );
}

#[test]
fn minus_after_keyword_is_literal() {
    // After a keyword part `after_expr` is false → `-3` is a negative literal
    assert_eq!(
        tokens("foo: -3"),
        vec![Token::Keyword("foo:".into()), Token::Integer(-3)]
    );
}

#[test]
fn minus_after_rparen_is_binary() {
    assert_eq!(
        tokens("(42) -3"),
        vec![
            Token::LParen,
            Token::Integer(42),
            Token::RParen,
            Token::Binary("-".into()),
            Token::Integer(3)
        ]
    );
}

#[test]
fn minus_after_rbracket_is_binary() {
    assert_eq!(
        tokens("[42] -1"),
        vec![
            Token::LBrack,
            Token::Integer(42),
            Token::RBrack,
            Token::Binary("-".into()),
            Token::Integer(1)
        ]
    );
}

#[test]
fn lt_space_minus_digit_is_binary_then_literal() {
    // `< -3`: space separates `<` and `-`, so `<` is its own binary selector
    // and `-3` is a negative literal (after_expr = false after a binary selector)
    assert_eq!(
        tokens("< -3"),
        vec![Token::Binary("<".into()), Token::Integer(-3)]
    );
}

// ── Multi-token sequences ──────────────────────────────────────────────────

#[test]
fn sequence_ident_message_ident() {
    assert_eq!(
        tokens("self printString"),
        vec![Token::Self_, Token::Ident("printString".into())]
    );
}

#[test]
fn var_slot_declaration() {
    // `x <- 0` — `<-` is a binary selector; parser distinguishes var decl by context
    assert_eq!(
        tokens("x <- 0"),
        vec![Token::Ident("x".into()), Token::Binary("<-".into()), Token::Integer(0)]
    );
}

#[test]
fn object_literal_delimiters() {
    // `(| x = 42 |)` — the `|` and `=` are binary selectors
    assert_eq!(
        tokens("(| x = 42 |)"),
        vec![
            Token::LParen,
            Token::Binary("|".into()),
            Token::Ident("x".into()),
            Token::Binary("=".into()),
            Token::Integer(42),
            Token::Binary("|".into()),
            Token::RParen,
        ]
    );
}

#[test]
fn block_with_arg() {
    // `[| :x | x + 1]`
    assert_eq!(
        tokens("[| :x | x + 1]"),
        vec![
            Token::LBrack,
            Token::Binary("|".into()),
            Token::Colon,
            Token::Ident("x".into()),
            Token::Binary("|".into()),
            Token::Ident("x".into()),
            Token::Binary("+".into()),
            Token::Integer(1),
            Token::RBrack,
        ]
    );
}

#[test]
fn cascade_semicolon() {
    // `foo bar; baz`
    assert_eq!(
        tokens("foo bar; baz"),
        vec![
            Token::Ident("foo".into()),
            Token::Ident("bar".into()),
            Token::Semi,
            Token::Ident("baz".into()),
        ]
    );
}

#[test]
fn non_local_return() {
    assert_eq!(
        tokens("^ self"),
        vec![Token::Caret, Token::Self_]
    );
}

#[test]
fn annotation_delimiters() {
    // `{} = 'doc'.`
    assert_eq!(
        tokens("{} = 'doc'."),
        vec![
            Token::LBrace,
            Token::RBrace,
            Token::Binary("=".into()),
            Token::Str("doc".into()),
            Token::Dot,
        ]
    );
}

// ── Error cases ────────────────────────────────────────────────────────────

#[test]
fn unexpected_char_error() {
    assert!(lex_err("\\").contains("unexpected character"));
    assert!(lex_err("#").contains("unexpected character"));
}
