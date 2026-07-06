use std::collections::HashMap;
use std::rc::Rc;

use crate::arena::{Arena, ObjectId};
use crate::error::{EgoError, SourceSpan};
use crate::gc::{alloc_with_gc, collect, RootSet};
use crate::object::{Object, ObjectKind};

pub type PrimFn = fn(
    recv: ObjectId,
    args: &[ObjectId],
    arena: &mut Arena,
    roots: &mut RootSet,
) -> Result<ObjectId, EgoError>;

pub struct PrimitiveTable {
    map: HashMap<&'static str, PrimFn>,
}

impl PrimitiveTable {
    pub fn new() -> Self {
        Self { map: HashMap::new() }
    }

    pub fn register(&mut self, sel: &'static str, f: PrimFn) {
        self.map.insert(sel, f);
    }

    pub fn get(&self, sel: &str) -> Option<PrimFn> {
        self.map.get(sel).copied()
    }
}

fn prim_span() -> SourceSpan {
    SourceSpan::new(Rc::new("<primitive>".into()), 0, 0)
}

fn prim_int_print_string(
    recv: ObjectId,
    _args: &[ObjectId],
    arena: &mut Arena,
    roots: &mut RootSet,
) -> Result<ObjectId, EgoError> {
    let n = match arena.get(recv).kind {
        ObjectKind::Integer(n) => n,
        _ => return Err(EgoError::new(prim_span(), "_IntPrintString requires integer receiver".into())),
    };
    let s: Box<str> = n.to_string().into_boxed_str();
    Ok(alloc_with_gc(arena, roots, Object::new(ObjectKind::StringVal(s))))
}

fn prim_float_print_string(
    recv: ObjectId,
    _args: &[ObjectId],
    arena: &mut Arena,
    roots: &mut RootSet,
) -> Result<ObjectId, EgoError> {
    let f = match arena.get(recv).kind {
        ObjectKind::Float(f) => f,
        _ => return Err(EgoError::new(prim_span(), "_FloatPrintString requires float receiver".into())),
    };
    let s = format_float(f);
    Ok(alloc_with_gc(arena, roots, Object::new(ObjectKind::StringVal(s.into_boxed_str()))))
}

fn format_float(f: f64) -> String {
    if f.is_nan() { return "nan".to_string(); }
    if f.is_infinite() {
        return if f > 0.0 { "inf".to_string() } else { "-inf".to_string() };
    }
    let s = format!("{f}");
    if s.contains('.') || s.contains('e') || s.contains('E') { s } else { format!("{s}.0") }
}

fn prim_print_line(
    _recv: ObjectId,
    args: &[ObjectId],
    arena: &mut Arena,
    roots: &mut RootSet,
) -> Result<ObjectId, EgoError> {
    if args.is_empty() {
        return Err(EgoError::new(prim_span(), "_PrintLine: requires an argument".into()));
    }
    match &arena.get(args[0]).kind {
        ObjectKind::StringVal(s) => {
            println!("{s}");
            Ok(roots.nil_id)
        }
        _ => Err(EgoError::new(prim_span(), "_PrintLine: requires a string argument".into())),
    }
}

fn prim_gc_collect(
    _recv: ObjectId,
    _args: &[ObjectId],
    arena: &mut Arena,
    roots: &mut RootSet,
) -> Result<ObjectId, EgoError> {
    collect(arena, roots);
    Ok(roots.nil_id)
}

pub fn register_all(prims: &mut PrimitiveTable) {
    prims.register("_IntPrintString", prim_int_print_string);
    prims.register("_FloatPrintString", prim_float_print_string);
    prims.register("_PrintLine:", prim_print_line);
    prims.register("_GcCollect", prim_gc_collect);
}
