use std::collections::HashSet;
use std::rc::Rc;

use crate::arena::{Arena, ObjectId};
use crate::ast::{Expr, ExprKind, Stmt, StmtKind};
use crate::env::ActivationId;
use crate::error::{EgoError, SourceSpan};
use crate::gc::{alloc_with_gc, RootSet};
use crate::lexer::lex;
use crate::object::{MethodDef, Object, ObjectKind, Slot, SlotKind};
use crate::parser::parse;
use crate::primitives::PrimitiveTable;

pub struct Interpreter {
    pub arena: Arena,
    pub roots: RootSet,
    pub prims: PrimitiveTable,
    pub(crate) activation_counter: u64,
    pub(crate) live_activations: HashSet<ActivationId>,
}

impl Interpreter {
    pub fn next_activation_id(&mut self) -> ActivationId {
        let id = ActivationId(self.activation_counter);
        self.activation_counter += 1;
        id
    }
}

fn bs_span() -> SourceSpan {
    SourceSpan::new(Rc::new("<bootstrap>".into()), 0, 0)
}

fn make_unary_prim_method(prim_sel: &str, arena: &mut Arena, roots: &RootSet) -> ObjectId {
    let span = bs_span();
    let body = vec![Stmt {
        kind: StmtKind::Expr(Box::new(Expr {
            kind: ExprKind::UnarySend {
                recv: Box::new(Expr { kind: ExprKind::Self_, span: span.clone() }),
                sel: prim_sel.to_string(),
            },
            span: span.clone(),
        })),
        span: span.clone(),
    }];
    let method_def = Rc::new(MethodDef { params: vec![], body, source: span });
    alloc_with_gc(arena, roots, Object::new(ObjectKind::Method(method_def)))
}

/// A one-argument method (backs binary and single-keyword selectors alike)
/// whose body forwards `self` and the argument to a primitive.
fn make_binary_prim_method(prim_sel: &str, arena: &mut Arena, roots: &RootSet) -> ObjectId {
    let span = bs_span();
    let body = vec![Stmt {
        kind: StmtKind::Expr(Box::new(Expr {
            kind: ExprKind::KeywordSend {
                recv: Box::new(Expr { kind: ExprKind::Self_, span: span.clone() }),
                sel: prim_sel.to_string(),
                args: vec![Expr { kind: ExprKind::Ident("other".to_string()), span: span.clone() }],
            },
            span: span.clone(),
        })),
        span: span.clone(),
    }];
    let method_def = Rc::new(MethodDef { params: vec!["other".to_string()], body, source: span });
    alloc_with_gc(arena, roots, Object::new(ObjectKind::Method(method_def)))
}

/// A two-argument method (backs `value:With:`) whose body forwards `self`
/// and both arguments to a primitive.
fn make_two_arg_prim_method(prim_sel: &str, arena: &mut Arena, roots: &RootSet) -> ObjectId {
    let span = bs_span();
    let body = vec![Stmt {
        kind: StmtKind::Expr(Box::new(Expr {
            kind: ExprKind::KeywordSend {
                recv: Box::new(Expr { kind: ExprKind::Self_, span: span.clone() }),
                sel: prim_sel.to_string(),
                args: vec![
                    Expr { kind: ExprKind::Ident("a".to_string()), span: span.clone() },
                    Expr { kind: ExprKind::Ident("b".to_string()), span: span.clone() },
                ],
            },
            span: span.clone(),
        })),
        span: span.clone(),
    }];
    let method_def = Rc::new(MethodDef {
        params: vec!["a".to_string(), "b".to_string()],
        body,
        source: span,
    });
    alloc_with_gc(arena, roots, Object::new(ObjectKind::Method(method_def)))
}

fn make_const_string_method(s: &str, arena: &mut Arena, roots: &RootSet) -> ObjectId {
    let span = bs_span();
    let body = vec![Stmt {
        kind: StmtKind::Expr(Box::new(Expr { kind: ExprKind::Str(s.to_string()), span: span.clone() })),
        span: span.clone(),
    }];
    let method_def = Rc::new(MethodDef { params: vec![], body, source: span });
    alloc_with_gc(arena, roots, Object::new(ObjectKind::Method(method_def)))
}

/// Builds a trait object with unary methods (each forwarding to a zero-arg
/// primitive) and binary/keyword-shaped methods (each forwarding `self` and
/// one argument to a one-arg primitive).
fn make_trait(
    unary_methods: &[(&str, &str)],
    binary_methods: &[(&str, &str)],
    arena: &mut Arena,
    roots: &RootSet,
) -> ObjectId {
    let mut trait_obj = Object::new(ObjectKind::Plain);
    for &(name, prim_sel) in unary_methods {
        let method_obj = make_unary_prim_method(prim_sel, arena, roots);
        trait_obj.slots.push(Slot { name: name.to_string(), kind: SlotKind::Method, value: method_obj });
    }
    for &(name, prim_sel) in binary_methods {
        let method_obj = make_binary_prim_method(prim_sel, arena, roots);
        trait_obj.slots.push(Slot { name: name.to_string(), kind: SlotKind::Method, value: method_obj });
    }
    alloc_with_gc(arena, roots, trait_obj)
}

pub fn bootstrap() -> Result<Interpreter, EgoError> {
    let mut arena = Arena::new();
    let mut roots = RootSet::new();

    // Step 1-2: permanent objects
    let nil_id        = arena.alloc(Object::new(ObjectKind::Plain));
    let true_id       = arena.alloc(Object::new(ObjectKind::Plain));
    let false_id      = arena.alloc(Object::new(ObjectKind::Plain));
    let integer_proto = arena.alloc(Object::new(ObjectKind::Plain));
    let float_proto   = arena.alloc(Object::new(ObjectKind::Plain));
    let string_proto  = arena.alloc(Object::new(ObjectKind::Plain));
    let block_proto   = arena.alloc(Object::new(ObjectKind::Plain));

    roots.nil_id        = nil_id;
    roots.true_id       = true_id;
    roots.false_id      = false_id;
    roots.integer_proto = integer_proto;
    roots.float_proto   = float_proto;
    roots.string_proto  = string_proto;
    roots.block_proto   = block_proto;

    // Step 3: primitives
    let mut prims = PrimitiveTable::new();
    crate::primitives::register_all(&mut prims);

    // Step 4-5: lobby
    let mut lobby = Object::new(ObjectKind::Plain);
    for (name, id) in [
        ("nil",          nil_id),
        ("true",         true_id),
        ("false",        false_id),
        ("integerProto", integer_proto),
        ("floatProto",   float_proto),
        ("stringProto",  string_proto),
        ("blockProto",   block_proto),
    ] {
        lobby.slots.push(Slot { name: name.to_string(), kind: SlotKind::Data, value: id });
    }
    let lobby_id = arena.alloc(lobby);
    roots.lobby = lobby_id;

    // Wire numeric traits and boolean printString via inline Rust-hardcoded
    // traits. (Moving this into boot.ego needs mirror-based reflection,
    // substage 1.16, since boot.ego has no way to attach methods to an
    // already-allocated prototype object before then.)
    let int_trait = make_trait(
        &[("printString", "_IntPrintString")],
        &[
            ("+", "_IntAdd:"), ("-", "_IntSub:"), ("*", "_IntMul:"), ("/", "_IntDiv:"),
            ("<", "_IntLt:"), ("<=", "_IntLe:"), (">", "_IntGt:"), (">=", "_IntGe:"),
            ("=", "_IntEq:"), ("~=", "_IntNe:"),
        ],
        &mut arena, &roots,
    );
    arena.get_mut(integer_proto).slots.push(Slot {
        name: "parent*".to_string(),
        kind: SlotKind::Parent,
        value: int_trait,
    });

    let float_trait = make_trait(
        &[("printString", "_FloatPrintString")],
        &[
            ("+", "_FloatAdd:"), ("-", "_FloatSub:"), ("*", "_FloatMul:"), ("/", "_FloatDiv:"),
            ("<", "_FloatLt:"), ("<=", "_FloatLe:"), (">", "_FloatGt:"), (">=", "_FloatGe:"),
            ("=", "_FloatEq:"), ("~=", "_FloatNe:"),
        ],
        &mut arena, &roots,
    );
    arena.get_mut(float_proto).slots.push(Slot {
        name: "parent*".to_string(),
        kind: SlotKind::Parent,
        value: float_trait,
    });

    let value_method = make_unary_prim_method("_BlockValue", &mut arena, &roots);
    let value_1_method = make_binary_prim_method("_BlockValue:", &mut arena, &roots);
    let value_2_method = make_two_arg_prim_method("_BlockValue:Value:", &mut arena, &roots);
    let mut block_trait_obj = Object::new(ObjectKind::Plain);
    block_trait_obj.slots.push(Slot { name: "value".to_string(), kind: SlotKind::Method, value: value_method });
    block_trait_obj.slots.push(Slot { name: "value:".to_string(), kind: SlotKind::Method, value: value_1_method });
    block_trait_obj.slots.push(Slot { name: "value:With:".to_string(), kind: SlotKind::Method, value: value_2_method });
    let block_trait = alloc_with_gc(&mut arena, &roots, block_trait_obj);
    arena.get_mut(block_proto).slots.push(Slot {
        name: "parent*".to_string(),
        kind: SlotKind::Parent,
        value: block_trait,
    });

    let true_print = make_const_string_method("true", &mut arena, &roots);
    arena.get_mut(true_id).slots.push(Slot {
        name: "printString".to_string(),
        kind: SlotKind::Method,
        value: true_print,
    });

    let false_print = make_const_string_method("false", &mut arena, &roots);
    arena.get_mut(false_id).slots.push(Slot {
        name: "printString".to_string(),
        kind: SlotKind::Method,
        value: false_print,
    });

    let mut interp = Interpreter {
        arena,
        roots,
        prims,
        activation_counter: 0,
        live_activations: HashSet::new(),
    };

    // Step 6: load boot.ego (currently just a comment; safe to parse+eval)
    let boot_src = include_str!("../../boot/boot.ego");
    let boot_file = Rc::new("boot.ego".to_string());
    let tokens = lex(boot_src, boot_file.clone())?;
    let program = parse(&tokens, boot_file)?;
    if !program.is_empty() {
        let lobby = interp.roots.lobby;
        crate::eval::eval_program(&program, lobby, &mut interp).map_err(|sig| match sig {
            crate::eval::EgoSignal::Err(e) => e,
            crate::eval::EgoSignal::Exception(_) =>
                EgoError::new(bs_span(), "exception during boot.ego evaluation".into()),
            crate::eval::EgoSignal::NonLocalReturn(_, _) =>
                EgoError::new(bs_span(), "non-local return escaped boot.ego".into()),
        })?;
    }

    Ok(interp)
}
