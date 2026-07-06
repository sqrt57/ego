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
    pub integer_proto: ObjectId,
    pub float_proto: ObjectId,
    pub string_proto: ObjectId,
    pub block_proto: ObjectId,
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

fn make_trait_with_unary_method(
    method_name: &str,
    prim_sel: &str,
    arena: &mut Arena,
    roots: &RootSet,
) -> ObjectId {
    let method_obj = make_unary_prim_method(prim_sel, arena, roots);
    let mut trait_obj = Object::new(ObjectKind::Plain);
    trait_obj.slots.push(Slot {
        name: method_name.to_string(),
        kind: SlotKind::Method,
        value: method_obj,
    });
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

    roots.nil_id   = nil_id;
    roots.true_id  = true_id;
    roots.false_id = false_id;

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

    // Wire printString for integers and floats via inline traits.
    // (In later substages this moves to boot.ego once object-literal eval is ready.)
    let int_trait = make_trait_with_unary_method("printString", "_IntPrintString", &mut arena, &roots);
    arena.get_mut(integer_proto).slots.push(Slot {
        name: "parent*".to_string(),
        kind: SlotKind::Parent,
        value: int_trait,
    });

    let float_trait = make_trait_with_unary_method("printString", "_FloatPrintString", &mut arena, &roots);
    arena.get_mut(float_proto).slots.push(Slot {
        name: "parent*".to_string(),
        kind: SlotKind::Parent,
        value: float_trait,
    });

    let mut interp = Interpreter {
        arena,
        roots,
        prims,
        integer_proto,
        float_proto,
        string_proto,
        block_proto,
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
