use std::rc::Rc;

use rstest::rstest;
use treewalk::ast::*;
use treewalk::error::SourceSpan;
use treewalk::lexer::lex;
use treewalk::parser::parse;

// ── Helpers ────────────────────────────────────────────────────────────────

fn file() -> Rc<String> {
    Rc::new("<test>".to_string())
}

/// Dummy span used in expected values — all spans are stripped before comparison.
fn ds() -> SourceSpan {
    SourceSpan::new(Rc::new(String::new()), 0, 0)
}

/// Build an Expr with a dummy span.
fn ex(kind: ExprKind) -> Expr {
    Expr { kind, span: ds() }
}

/// Build a Stmt with a dummy span.
fn es(kind: StmtKind) -> Stmt {
    Stmt { kind, span: ds() }
}

// ── Span stripping ─────────────────────────────────────────────────────────

fn strip(stmts: Vec<Stmt>) -> Vec<Stmt> {
    stmts.into_iter().map(ss).collect()
}

fn ss(s: Stmt) -> Stmt {
    Stmt {
        kind: match s.kind {
            StmtKind::Return(e) => StmtKind::Return(Box::new(se(*e))),
            StmtKind::Expr(e)   => StmtKind::Expr(Box::new(se(*e))),
        },
        span: ds(),
    }
}

fn se(e: Expr) -> Expr {
    Expr { kind: sk(e.kind), span: ds() }
}

fn sk(k: ExprKind) -> ExprKind {
    match k {
        ExprKind::Int(_) | ExprKind::Float(_) | ExprKind::Str(_)
        | ExprKind::Ident(_) | ExprKind::Self_ => k,
        ExprKind::Assign { name, value } =>
            ExprKind::Assign { name, value: Box::new(se(*value)) },
        ExprKind::UnarySend { recv, sel } =>
            ExprKind::UnarySend { recv: Box::new(se(*recv)), sel },
        ExprKind::BinarySend { recv, sel, arg } =>
            ExprKind::BinarySend { recv: Box::new(se(*recv)), sel, arg: Box::new(se(*arg)) },
        ExprKind::KeywordSend { recv, sel, args } =>
            ExprKind::KeywordSend { recv: Box::new(se(*recv)), sel, args: args.into_iter().map(se).collect() },
        ExprKind::ResendSend { target, sel, args } =>
            ExprKind::ResendSend { target, sel, args: args.into_iter().map(se).collect() },
        ExprKind::Cascade { recv, msgs } =>
            ExprKind::Cascade { recv: Box::new(se(*recv)), msgs: msgs.into_iter().map(scm).collect() },
        ExprKind::Block(b)  => ExprKind::Block(Rc::new(sb((*b).clone()))),
        ExprKind::Object(o) => ExprKind::Object(Box::new(so(*o))),
    }
}

fn scm(m: CascadeMsg) -> CascadeMsg {
    match m {
        CascadeMsg::Unary   { sel, .. }       => CascadeMsg::Unary   { sel, span: ds() },
        CascadeMsg::Binary  { sel, arg, .. }  => CascadeMsg::Binary  { sel, arg: se(arg), span: ds() },
        CascadeMsg::Keyword { sel, args, .. } => CascadeMsg::Keyword { sel, args: args.into_iter().map(se).collect(), span: ds() },
    }
}

fn sb(b: BlockLit) -> BlockLit {
    BlockLit {
        params: b.params,
        locals: b.locals.into_iter().map(|l| BlockLocal { name: l.name, kind: l.kind, init: se(l.init) }).collect(),
        body:   b.body.into_iter().map(ss).collect(),
        span:   ds(),
    }
}

fn so(o: ObjectLit) -> ObjectLit {
    ObjectLit {
        annotation: o.annotation,
        slots:      o.slots.into_iter().map(sd).collect(),
        body:       o.body.into_iter().map(ss).collect(),
        span:       ds(),
    }
}

fn sd(s: SlotDecl) -> SlotDecl {
    SlotDecl {
        kind: match s.kind {
            SlotDeclKind::Data   { name, value } => SlotDeclKind::Data   { name, value: se(value) },
            SlotDeclKind::Var    { name, value } => SlotDeclKind::Var    { name, value: se(value) },
            SlotDeclKind::Arg    { name }        => SlotDeclKind::Arg    { name },
            SlotDeclKind::Parent { name, value } => SlotDeclKind::Parent { name, value: se(value) },
            SlotDeclKind::Method { sel, body }   => SlotDeclKind::Method { sel, body: body.into_iter().map(ss).collect() },
        },
        span: ds(),
    }
}

// ── Parse entry points ─────────────────────────────────────────────────────

fn parse_ok(src: &str) -> Vec<Stmt> {
    let toks = lex(src, file()).expect("lex error");
    strip(parse(&toks, file()).expect("parse error"))
}

fn parse_err(src: &str) -> String {
    let toks = lex(src, file()).expect("lex error");
    parse(&toks, file()).expect_err("expected a parse error but got Ok").message
}

/// Parse a single-expression program and return its ExprKind.
fn expr(src: &str) -> ExprKind {
    let stmts = parse_ok(src);
    assert_eq!(stmts.len(), 1, "expected one statement in {src:?}");
    match stmts.into_iter().next().unwrap().kind {
        StmtKind::Expr(e) => e.kind,
        StmtKind::Return(_) => panic!("expected Expr stmt, got Return"),
    }
}

/// Extract the ObjectLit from a single-expression program.
fn obj_of(stmts: &[Stmt]) -> ObjectLit {
    assert_eq!(stmts.len(), 1);
    match &stmts[0].kind {
        StmtKind::Expr(e) => match &e.kind {
            ExprKind::Object(o) => *o.clone(),
            _ => panic!("expected Object expr, got {:?}", e.kind),
        },
        _ => panic!("expected Expr stmt"),
    }
}

// ── Literals ───────────────────────────────────────────────────────────────

#[rstest]
#[case("0",   ExprKind::Int(0))]
#[case("42",  ExprKind::Int(42))]
#[case("-7",  ExprKind::Int(-7))]
fn int_literal(#[case] src: &str, #[case] expected: ExprKind) {
    assert_eq!(expr(src), expected);
}

#[rstest]
#[case("3.14",  ExprKind::Float(3.14))]
#[case("-1.5",  ExprKind::Float(-1.5))]
fn float_literal(#[case] src: &str, #[case] expected: ExprKind) {
    assert_eq!(expr(src), expected);
}

#[rstest]
#[case("'hello'",  ExprKind::Str("hello".into()))]
#[case("''",       ExprKind::Str("".into()))]
#[case("'it''s'",  ExprKind::Str("it's".into()))]
fn string_literal(#[case] src: &str, #[case] expected: ExprKind) {
    assert_eq!(expr(src), expected);
}

// ── Variables ──────────────────────────────────────────────────────────────

#[test]
fn ident() {
    assert_eq!(expr("foo"), ExprKind::Ident("foo".into()));
}

#[test]
fn self_expr() {
    assert_eq!(expr("self"), ExprKind::Self_);
}

// ── Resend ─────────────────────────────────────────────────────────────────

#[test]
fn resend_unary() {
    assert_eq!(
        expr("resend.foo"),
        ExprKind::ResendSend { target: ResendTarget::Undirected, sel: "foo".into(), args: vec![] }
    );
}

#[test]
fn resend_binary() {
    assert_eq!(
        expr("resend.+ 5"),
        ExprKind::ResendSend {
            target: ResendTarget::Undirected,
            sel: "+".into(),
            args: vec![ex(ExprKind::Int(5))],
        }
    );
}

#[test]
fn resend_keyword() {
    assert_eq!(
        expr("resend.min: 17 Max: 23"),
        ExprKind::ResendSend {
            target: ResendTarget::Undirected,
            sel: "min:Max:".into(),
            args: vec![ex(ExprKind::Int(17)), ex(ExprKind::Int(23))],
        }
    );
}

#[test]
fn directed_resend() {
    assert_eq!(
        expr("intParent.min: 17"),
        ExprKind::ResendSend {
            target: ResendTarget::Directed("intParent".into()),
            sel: "min:".into(),
            args: vec![ex(ExprKind::Int(17))],
        }
    );
}

#[test]
fn resend_result_can_be_unary_chained() {
    // resend.foo bar  →  (resend.foo) bar
    let inner = ex(ExprKind::ResendSend {
        target: ResendTarget::Undirected,
        sel: "foo".into(),
        args: vec![],
    });
    assert_eq!(
        expr("resend.foo bar"),
        ExprKind::UnarySend { recv: Box::new(inner), sel: "bar".into() }
    );
}

#[test]
fn bare_resend_is_an_error() {
    let msg = parse_err("resend");
    assert!(msg.contains("resend"), "got: {msg}");
}

#[test]
fn resend_with_space_before_dot_is_an_error() {
    // The dot only counts as a resend dot with no whitespace on either side;
    // `resend .foo` lexes as [Resend, Dot, Ident], and a bare `resend` isn't
    // a valid expression on its own.
    let msg = parse_err("resend .foo");
    assert!(msg.contains("resend"), "got: {msg}");
}

#[test]
fn resend_dot_without_message_is_an_error() {
    let msg = parse_err("resend.");
    assert!(!msg.is_empty());
}

// ── Unary sends ────────────────────────────────────────────────────────────

#[test]
fn unary_send() {
    assert_eq!(
        expr("a foo"),
        ExprKind::UnarySend {
            recv: Box::new(ex(ExprKind::Ident("a".into()))),
            sel: "foo".into(),
        }
    );
}

#[test]
fn unary_send_chain() {
    // a foo bar  →  (a foo) bar
    let inner = ex(ExprKind::UnarySend {
        recv: Box::new(ex(ExprKind::Ident("a".into()))),
        sel: "foo".into(),
    });
    assert_eq!(
        expr("a foo bar"),
        ExprKind::UnarySend { recv: Box::new(inner), sel: "bar".into() }
    );
}

// ── Binary sends ───────────────────────────────────────────────────────────

#[test]
fn binary_send() {
    assert_eq!(
        expr("a + b"),
        ExprKind::BinarySend {
            recv: Box::new(ex(ExprKind::Ident("a".into()))),
            sel: "+".into(),
            arg: Box::new(ex(ExprKind::Ident("b".into()))),
        }
    );
}

#[test]
fn binary_send_chain() {
    // a + b + c  →  (a + b) + c  — repeating the *same* operator associates left to right.
    let inner = ex(ExprKind::BinarySend {
        recv: Box::new(ex(ExprKind::Ident("a".into()))),
        sel: "+".into(),
        arg: Box::new(ex(ExprKind::Ident("b".into()))),
    });
    assert_eq!(
        expr("a + b + c"),
        ExprKind::BinarySend { recv: Box::new(inner), sel: "+".into(), arg: Box::new(ex(ExprKind::Ident("c".into()))) }
    );
}

#[test]
fn error_mixed_binary_operators() {
    // a + b - c mixes '+' and '-' without parentheses — not allowed.
    let msg = parse_err("a + b - c");
    assert!(msg.contains('+') && msg.contains('-'), "got: {msg}");
}

// ── Keyword sends ──────────────────────────────────────────────────────────

#[test]
fn keyword_send_single() {
    assert_eq!(
        expr("a foo: 1"),
        ExprKind::KeywordSend {
            recv: Box::new(ex(ExprKind::Ident("a".into()))),
            sel: "foo:".into(),
            args: vec![ex(ExprKind::Int(1))],
        }
    );
}

#[test]
fn keyword_send_multi() {
    // Two cap-continued parts concatenate into one selector: at:Put:.
    assert_eq!(
        expr("a at: 1 Put: 2"),
        ExprKind::KeywordSend {
            recv: Box::new(ex(ExprKind::Ident("a".into()))),
            sel: "at:Put:".into(),
            args: vec![ex(ExprKind::Int(1)), ex(ExprKind::Int(2))],
        }
    );
}

#[test]
fn keyword_lowercase_part_nests() {
    // a at: 1 put: 2  →  a at: (1 put: 2)
    // A lowercase-initial part after the first doesn't continue "at:" — it
    // closes that message and starts a new one, nested as its final argument.
    assert_eq!(
        expr("a at: 1 put: 2"),
        ExprKind::KeywordSend {
            recv: Box::new(ex(ExprKind::Ident("a".into()))),
            sel: "at:".into(),
            args: vec![ex(ExprKind::KeywordSend {
                recv: Box::new(ex(ExprKind::Int(1))),
                sel: "put:".into(),
                args: vec![ex(ExprKind::Int(2))],
            })],
        }
    );
}

#[test]
fn keyword_nesting_three_deep() {
    // 5 min: 6 min: 7 Max: 8 Max: 9 min: 10 Max: 11
    // = 5 min: (6 min: 7 Max: 8 Max: (9 min: 10 Max: 11))
    assert_eq!(
        expr("5 min: 6 min: 7 Max: 8 Max: 9 min: 10 Max: 11"),
        ExprKind::KeywordSend {
            recv: Box::new(ex(ExprKind::Int(5))),
            sel: "min:".into(),
            args: vec![ex(ExprKind::KeywordSend {
                recv: Box::new(ex(ExprKind::Int(6))),
                sel: "min:Max:Max:".into(),
                args: vec![
                    ex(ExprKind::Int(7)),
                    ex(ExprKind::Int(8)),
                    ex(ExprKind::KeywordSend {
                        recv: Box::new(ex(ExprKind::Int(9))),
                        sel: "min:Max:".into(),
                        args: vec![ex(ExprKind::Int(10)), ex(ExprKind::Int(11))],
                    }),
                ],
            })],
        }
    );
}

#[test]
fn error_keyword_message_cannot_start_with_cap_part() {
    // A cap-initial part may only continue a message already in progress —
    // it can never start one, so "At:" here is left unconsumed and the
    // statement parser trips over it looking for a '.'.
    let msg = parse_err("a At: 1 Put: 2");
    assert!(!msg.is_empty());
}

#[test]
fn mixed_keyword_send() {
    // Small and cap keyword parts accumulate into one selector.
    assert_eq!(
        expr("a at: 1 Put: 2"),
        ExprKind::KeywordSend {
            recv: Box::new(ex(ExprKind::Ident("a".into()))),
            sel: "at:Put:".into(),
            args: vec![ex(ExprKind::Int(1)), ex(ExprKind::Int(2))],
        }
    );
}

// ── Precedence ─────────────────────────────────────────────────────────────

#[test]
fn unary_higher_than_binary() {
    // a + b foo  →  a + (b foo)
    assert_eq!(
        expr("a + b foo"),
        ExprKind::BinarySend {
            recv: Box::new(ex(ExprKind::Ident("a".into()))),
            sel: "+".into(),
            arg: Box::new(ex(ExprKind::UnarySend {
                recv: Box::new(ex(ExprKind::Ident("b".into()))),
                sel: "foo".into(),
            })),
        }
    );
}

#[test]
fn binary_higher_than_keyword() {
    // a foo: b + c  →  a foo: (b + c)
    assert_eq!(
        expr("a foo: b + c"),
        ExprKind::KeywordSend {
            recv: Box::new(ex(ExprKind::Ident("a".into()))),
            sel: "foo:".into(),
            args: vec![ex(ExprKind::BinarySend {
                recv: Box::new(ex(ExprKind::Ident("b".into()))),
                sel: "+".into(),
                arg: Box::new(ex(ExprKind::Ident("c".into()))),
            })],
        }
    );
}

#[test]
fn parens_override_precedence() {
    // (a + b) foo
    assert_eq!(
        expr("(a + b) foo"),
        ExprKind::UnarySend {
            recv: Box::new(ex(ExprKind::BinarySend {
                recv: Box::new(ex(ExprKind::Ident("a".into()))),
                sel: "+".into(),
                arg: Box::new(ex(ExprKind::Ident("b".into()))),
            })),
            sel: "foo".into(),
        }
    );
}

// ── Return statements ──────────────────────────────────────────────────────

#[test]
fn return_stmt() {
    assert_eq!(
        parse_ok("^ x + 1"),
        vec![es(StmtKind::Return(Box::new(ex(ExprKind::BinarySend {
            recv: Box::new(ex(ExprKind::Ident("x".into()))),
            sel: "+".into(),
            arg: Box::new(ex(ExprKind::Int(1))),
        }))))]
    );
}

// ── Multi-statement programs ───────────────────────────────────────────────

#[test]
fn empty_program() {
    assert_eq!(parse_ok(""), vec![]);
}

#[test]
fn multi_stmt() {
    let stmts = parse_ok("a. b. c");
    assert_eq!(stmts.len(), 3);
    assert_eq!(stmts[0], es(StmtKind::Expr(Box::new(ex(ExprKind::Ident("a".into()))))));
    assert_eq!(stmts[1], es(StmtKind::Expr(Box::new(ex(ExprKind::Ident("b".into()))))));
    assert_eq!(stmts[2], es(StmtKind::Expr(Box::new(ex(ExprKind::Ident("c".into()))))));
}

#[test]
fn trailing_dot_ok() {
    let stmts = parse_ok("a.");
    assert_eq!(stmts, vec![es(StmtKind::Expr(Box::new(ex(ExprKind::Ident("a".into())))))]);
}

// ── Cascades ───────────────────────────────────────────────────────────────

#[test]
fn cascade_unary_msg() {
    // a foo; bar  — both "foo" and "bar" go to "a"
    assert_eq!(
        expr("a foo; bar"),
        ExprKind::Cascade {
            recv: Box::new(ex(ExprKind::Ident("a".into()))),
            msgs: vec![
                CascadeMsg::Unary { sel: "foo".into(), span: ds() },
                CascadeMsg::Unary { sel: "bar".into(), span: ds() },
            ],
        }
    );
}

#[test]
fn cascade_binary_msg() {
    // a + 1; - 2  — both "+ 1" and "- 2" go to "a"
    assert_eq!(
        expr("a + 1; - 2"),
        ExprKind::Cascade {
            recv: Box::new(ex(ExprKind::Ident("a".into()))),
            msgs: vec![
                CascadeMsg::Binary { sel: "+".into(), arg: ex(ExprKind::Int(1)), span: ds() },
                CascadeMsg::Binary { sel: "-".into(), arg: ex(ExprKind::Int(2)), span: ds() },
            ],
        }
    );
}

#[test]
fn cascade_keyword_msg() {
    // a foo; bar: 2  — both "foo" and "bar: 2" go to "a"
    assert_eq!(
        expr("a foo; bar: 2"),
        ExprKind::Cascade {
            recv: Box::new(ex(ExprKind::Ident("a".into()))),
            msgs: vec![
                CascadeMsg::Unary { sel: "foo".into(), span: ds() },
                CascadeMsg::Keyword { sel: "bar:".into(), args: vec![ex(ExprKind::Int(2))], span: ds() },
            ],
        }
    );
}

#[test]
fn cascade_multi_msg() {
    // a foo; bar; + 1  — three messages, all sent to "a"
    let stmts = parse_ok("a foo; bar; + 1");
    match &stmts[0].kind {
        StmtKind::Expr(e) => match &e.kind {
            ExprKind::Cascade { recv, msgs } => {
                assert_eq!(**recv, ex(ExprKind::Ident("a".into())));
                assert_eq!(msgs.len(), 3);
            }
            _ => panic!("expected Cascade"),
        },
        _ => panic!("expected Expr stmt"),
    }
}

#[test]
fn cascade_no_leading_send() {
    // a; foo  — "a" has no leading send to peel, so it is the receiver as-is
    assert_eq!(
        expr("a; foo"),
        ExprKind::Cascade {
            recv: Box::new(ex(ExprKind::Ident("a".into()))),
            msgs: vec![CascadeMsg::Unary { sel: "foo".into(), span: ds() }],
        }
    );
}

// ── Blocks ─────────────────────────────────────────────────────────────────

#[test]
fn block_empty() {
    assert_eq!(
        expr("[]"),
        ExprKind::Block(Rc::new(BlockLit { params: vec![], locals: vec![], body: vec![], span: ds() }))
    );
}

#[test]
fn block_body_only() {
    assert_eq!(
        expr("[1 + 2]"),
        ExprKind::Block(Rc::new(BlockLit {
            params: vec![],
            locals: vec![],
            body: vec![es(StmtKind::Expr(Box::new(ex(ExprKind::BinarySend {
                recv: Box::new(ex(ExprKind::Int(1))),
                sel: "+".into(),
                arg: Box::new(ex(ExprKind::Int(2))),
            }))))],
            span: ds(),
        }))
    );
}

#[test]
fn block_with_param() {
    // [| :x | x + 1]
    assert_eq!(
        expr("[| :x | x + 1]"),
        ExprKind::Block(Rc::new(BlockLit {
            params: vec!["x".into()],
            locals: vec![],
            body: vec![es(StmtKind::Expr(Box::new(ex(ExprKind::BinarySend {
                recv: Box::new(ex(ExprKind::Ident("x".into()))),
                sel: "+".into(),
                arg: Box::new(ex(ExprKind::Int(1))),
            }))))],
            span: ds(),
        }))
    );
}

#[test]
fn block_with_data_local() {
    // [| x = 0. | x]
    assert_eq!(
        expr("[| x = 0. | x]"),
        ExprKind::Block(Rc::new(BlockLit {
            params: vec![],
            locals: vec![BlockLocal { name: "x".into(), kind: LocalKind::Data, init: ex(ExprKind::Int(0)) }],
            body: vec![es(StmtKind::Expr(Box::new(ex(ExprKind::Ident("x".into())))))],
            span: ds(),
        }))
    );
}

#[test]
fn block_with_var_local() {
    // [| x <- 0. | x]
    assert_eq!(
        expr("[| x <- 0. | x]"),
        ExprKind::Block(Rc::new(BlockLit {
            params: vec![],
            locals: vec![BlockLocal { name: "x".into(), kind: LocalKind::Var, init: ex(ExprKind::Int(0)) }],
            body: vec![es(StmtKind::Expr(Box::new(ex(ExprKind::Ident("x".into())))))],
            span: ds(),
        }))
    );
}

#[test]
fn block_param_and_local() {
    // [| :x. y = 0. | x + y]
    let result = expr("[| :x. y = 0. | x + y]");
    match result {
        ExprKind::Block(b) => {
            assert_eq!(b.params, vec!["x".to_string()]);
            assert_eq!(b.locals.len(), 1);
            assert_eq!(b.locals[0].name, "y");
            assert_eq!(b.locals[0].kind, LocalKind::Data);
            assert_eq!(b.body.len(), 1);
        }
        _ => panic!("expected Block"),
    }
}

// ── Object literals ────────────────────────────────────────────────────────

#[test]
fn object_empty() {
    assert_eq!(
        expr("(| |)"),
        ExprKind::Object(Box::new(ObjectLit { annotation: None, slots: vec![], body: vec![], span: ds() }))
    );
}

#[test]
fn object_with_data_slot() {
    assert_eq!(
        expr("(| x = 1. |)"),
        ExprKind::Object(Box::new(ObjectLit {
            annotation: None,
            slots: vec![SlotDecl { kind: SlotDeclKind::Data { name: "x".into(), value: ex(ExprKind::Int(1)) }, span: ds() }],
            body: vec![],
            span: ds(),
        }))
    );
}

#[test]
fn object_with_body() {
    // (| x = 1. | x)
    let stmts = parse_ok("(| x = 1. | x)");
    let obj = obj_of(&stmts);
    assert_eq!(obj.slots.len(), 1);
    assert_eq!(obj.body.len(), 1);
}

#[test]
fn object_no_slots_with_body() {
    // (| | x + 1)
    let stmts = parse_ok("(| | x + 1)");
    let obj = obj_of(&stmts);
    assert_eq!(obj.slots.len(), 0);
    assert_eq!(obj.body.len(), 1);
}

// ── Slot kinds ─────────────────────────────────────────────────────────────

#[test]
fn slot_data_simple() {
    let stmts = parse_ok("(| x = 42. |)");
    let obj = obj_of(&stmts);
    assert_eq!(obj.slots[0].kind, SlotDeclKind::Data { name: "x".into(), value: ex(ExprKind::Int(42)) });
}

#[test]
fn slot_data_complex_value() {
    // Data slot value is a binary expression (no extra parens, so not a method).
    let stmts = parse_ok("(| x = a + b. |)");
    let obj = obj_of(&stmts);
    assert_eq!(
        obj.slots[0].kind,
        SlotDeclKind::Data {
            name: "x".into(),
            value: ex(ExprKind::BinarySend {
                recv: Box::new(ex(ExprKind::Ident("a".into()))),
                sel: "+".into(),
                arg: Box::new(ex(ExprKind::Ident("b".into()))),
            }),
        }
    );
}

#[test]
fn slot_var() {
    let stmts = parse_ok("(| x <- 0. |)");
    let obj = obj_of(&stmts);
    assert_eq!(obj.slots[0].kind, SlotDeclKind::Var { name: "x".into(), value: ex(ExprKind::Int(0)) });
}

#[test]
fn slot_arg() {
    let stmts = parse_ok("(| :x. |)");
    let obj = obj_of(&stmts);
    assert_eq!(obj.slots[0].kind, SlotDeclKind::Arg { name: "x".into() });
}

#[test]
fn slot_parent() {
    let stmts = parse_ok("(| p* = obj. |)");
    let obj = obj_of(&stmts);
    assert_eq!(
        obj.slots[0].kind,
        SlotDeclKind::Parent { name: "p".into(), value: ex(ExprKind::Ident("obj".into())) }
    );
}

#[test]
fn slot_unary_method() {
    // (| foo = (42) |) — method returning 42; parens after = delimit the body
    let stmts = parse_ok("(| foo = (42) |)");
    let obj = obj_of(&stmts);
    assert_eq!(
        obj.slots[0].kind,
        SlotDeclKind::Method {
            sel: MethodSel::Unary("foo".into()),
            body: vec![es(StmtKind::Expr(Box::new(ex(ExprKind::Int(42)))))],
        }
    );
}

#[test]
fn slot_binary_method() {
    // (| + x = (x) |)
    let stmts = parse_ok("(| + x = (x) |)");
    let obj = obj_of(&stmts);
    assert_eq!(
        obj.slots[0].kind,
        SlotDeclKind::Method {
            sel: MethodSel::Binary("+".into(), "x".into()),
            body: vec![es(StmtKind::Expr(Box::new(ex(ExprKind::Ident("x".into())))))],
        }
    );
}

#[test]
fn slot_keyword_method() {
    // (| at: k put: v = (k) |)
    let stmts = parse_ok("(| at: k put: v = (k) |)");
    let obj = obj_of(&stmts);
    assert_eq!(
        obj.slots[0].kind,
        SlotDeclKind::Method {
            sel: MethodSel::Keyword(vec![("at:".into(), "k".into()), ("put:".into(), "v".into())]),
            body: vec![es(StmtKind::Expr(Box::new(ex(ExprKind::Ident("k".into())))))],
        }
    );
}

#[test]
fn slot_cap_keyword_method() {
    // (| At: k Put: v = (k) |)
    let stmts = parse_ok("(| At: k Put: v = (k) |)");
    let obj = obj_of(&stmts);
    assert_eq!(
        obj.slots[0].kind,
        SlotDeclKind::Method {
            sel: MethodSel::Keyword(vec![("At:".into(), "k".into()), ("Put:".into(), "v".into())]),
            body: vec![es(StmtKind::Expr(Box::new(ex(ExprKind::Ident("k".into())))))],
        }
    );
}

#[test]
fn slot_method_with_multi_stmt_body() {
    // (| foo = (a. b) |)
    let stmts = parse_ok("(| foo = (a. b) |)");
    let obj = obj_of(&stmts);
    match &obj.slots[0].kind {
        SlotDeclKind::Method { sel: MethodSel::Unary(s), body } => {
            assert_eq!(s, "foo");
            assert_eq!(body.len(), 2);
        }
        _ => panic!("expected unary method slot"),
    }
}

#[test]
fn slot_annotation() {
    let stmts = parse_ok("(| {} = 'my annotation'. x = 1. |)");
    let obj = obj_of(&stmts);
    assert_eq!(obj.annotation, Some("my annotation".into()));
    assert_eq!(obj.slots.len(), 1);
}

#[test]
fn multi_slot_object() {
    let stmts = parse_ok("(| x = 1. y = 2. |)");
    let obj = obj_of(&stmts);
    assert_eq!(obj.slots.len(), 2);
}

// ── Object as data slot value ───────────────────────────────────────────────

#[test]
fn data_slot_object_value() {
    // (| x = (| |). |) — data slot whose value is an empty object literal.
    // (| |) starts with | so it is an ObjectLit primary, not a method body.
    let stmts = parse_ok("(| x = (| |). |)");
    let obj = obj_of(&stmts);
    match &obj.slots[0].kind {
        SlotDeclKind::Data { name, value } => {
            assert_eq!(name, "x");
            assert!(matches!(value.kind, ExprKind::Object(_)));
        }
        _ => panic!("expected Data slot"),
    }
}

// ── Error cases ────────────────────────────────────────────────────────────

#[test]
fn error_unclosed_paren() {
    let msg = parse_err("(a + b");
    assert!(msg.contains("')'"), "got: {msg}");
}

#[test]
fn error_unclosed_bracket() {
    let msg = parse_err("[1 + 2");
    assert!(msg.contains("']'"), "got: {msg}");
}

#[test]
fn error_unclosed_object_missing_bar() {
    let msg = parse_err("(| x = 1.");
    assert!(msg.contains("'|'"), "got: {msg}");
}

#[test]
fn error_unclosed_object_missing_paren() {
    let msg = parse_err("(| x = 1. |");
    assert!(msg.contains("')'"), "got: {msg}");
}

#[test]
fn error_bad_slot_no_equals() {
    // 'x' without '=' is not a valid slot declaration
    let msg = parse_err("(| x |)");
    assert!(!msg.is_empty());
}

#[test]
fn error_missing_dot_between_stmts() {
    // After Int(1), Integer(2) is not a valid continuation and not a '.'
    let msg = parse_err("1 2");
    assert!(msg.contains("'.'"), "got: {msg}");
}

#[test]
fn error_empty_expression() {
    // '.' with nothing before it
    let msg = parse_err(".");
    assert!(msg.contains("expression"), "got: {msg}");
}

#[test]
fn error_bad_cascade_msg() {
    // ';' must be followed by a valid message
    let msg = parse_err("a foo; 42");
    assert!(!msg.is_empty());
}
