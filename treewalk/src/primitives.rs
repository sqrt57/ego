use std::collections::HashMap;
use std::rc::Rc;

use crate::arena::{Arena, ObjectId};
use crate::error::{EgoError, SourceSpan};
use crate::gc::{alloc_with_gc, collect, RootSet};
use crate::object::{Object, ObjectKind, Slot, SlotKind};

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

// ── Numeric arithmetic and comparison ──────────────────────────────────────

enum Num {
    Int(i64),
    Float(f64),
}

impl Num {
    fn as_f64(&self) -> f64 {
        match *self {
            Num::Int(n) => n as f64,
            Num::Float(f) => f,
        }
    }
}

fn as_int(id: ObjectId, arena: &Arena, ctx: &str) -> Result<i64, EgoError> {
    match arena.get(id).kind {
        ObjectKind::Integer(n) => Ok(n),
        _ => Err(EgoError::new(prim_span(), format!("{ctx} requires integer receiver"))),
    }
}

fn as_float(id: ObjectId, arena: &Arena, ctx: &str) -> Result<f64, EgoError> {
    match arena.get(id).kind {
        ObjectKind::Float(f) => Ok(f),
        _ => Err(EgoError::new(prim_span(), format!("{ctx} requires float receiver"))),
    }
}

fn as_num(id: ObjectId, arena: &Arena, ctx: &str) -> Result<Num, EgoError> {
    match arena.get(id).kind {
        ObjectKind::Integer(n) => Ok(Num::Int(n)),
        ObjectKind::Float(f) => Ok(Num::Float(f)),
        _ => Err(EgoError::new(prim_span(), format!("{ctx} requires a numeric argument"))),
    }
}

fn one_arg(args: &[ObjectId], ctx: &str) -> Result<ObjectId, EgoError> {
    args.first().copied().ok_or_else(|| EgoError::new(prim_span(), format!("{ctx} requires one argument")))
}

fn overflow_err() -> EgoError {
    EgoError::new(prim_span(), "integer overflow (bignum promotion not implemented until substage 1.17)".into())
}

fn div_zero_err() -> EgoError {
    EgoError::new(prim_span(), "division by zero".into())
}

fn make_int(n: i64, arena: &mut Arena, roots: &RootSet) -> ObjectId {
    let id = alloc_with_gc(arena, roots, Object::new(ObjectKind::Integer(n)));
    arena.get_mut(id).slots.push(Slot {
        name: "parent*".to_string(),
        kind: SlotKind::Parent,
        value: roots.integer_proto,
    });
    id
}

fn make_float(f: f64, arena: &mut Arena, roots: &RootSet) -> ObjectId {
    let id = alloc_with_gc(arena, roots, Object::new(ObjectKind::Float(f)));
    arena.get_mut(id).slots.push(Slot {
        name: "parent*".to_string(),
        kind: SlotKind::Parent,
        value: roots.float_proto,
    });
    id
}

fn cmp_result(b: bool, roots: &RootSet) -> ObjectId {
    if b { roots.true_id } else { roots.false_id }
}

fn int_arith(
    recv: ObjectId,
    args: &[ObjectId],
    arena: &mut Arena,
    roots: &mut RootSet,
    sel: &str,
    int_op: fn(i64, i64) -> Option<i64>,
    float_op: fn(f64, f64) -> f64,
) -> Result<ObjectId, EgoError> {
    let a = as_int(recv, arena, sel)?;
    let arg = one_arg(args, sel)?;
    match as_num(arg, arena, sel)? {
        Num::Int(b) => int_op(a, b).map(|s| make_int(s, arena, roots)).ok_or_else(overflow_err),
        Num::Float(b) => Ok(make_float(float_op(a as f64, b), arena, roots)),
    }
}

fn float_arith(
    recv: ObjectId,
    args: &[ObjectId],
    arena: &mut Arena,
    roots: &mut RootSet,
    sel: &str,
    op: fn(f64, f64) -> f64,
) -> Result<ObjectId, EgoError> {
    let a = as_float(recv, arena, sel)?;
    let arg = one_arg(args, sel)?;
    let b = as_num(arg, arena, sel)?.as_f64();
    Ok(make_float(op(a, b), arena, roots))
}

fn int_cmp(
    recv: ObjectId,
    args: &[ObjectId],
    arena: &Arena,
    roots: &RootSet,
    sel: &str,
    int_op: fn(i64, i64) -> bool,
    float_op: fn(f64, f64) -> bool,
) -> Result<ObjectId, EgoError> {
    let a = as_int(recv, arena, sel)?;
    let arg = one_arg(args, sel)?;
    let result = match as_num(arg, arena, sel)? {
        Num::Int(b) => int_op(a, b),
        Num::Float(b) => float_op(a as f64, b),
    };
    Ok(cmp_result(result, roots))
}

fn float_cmp(
    recv: ObjectId,
    args: &[ObjectId],
    arena: &Arena,
    roots: &RootSet,
    sel: &str,
    op: fn(f64, f64) -> bool,
) -> Result<ObjectId, EgoError> {
    let a = as_float(recv, arena, sel)?;
    let arg = one_arg(args, sel)?;
    let b = as_num(arg, arena, sel)?.as_f64();
    Ok(cmp_result(op(a, b), roots))
}

fn prim_int_add(recv: ObjectId, args: &[ObjectId], arena: &mut Arena, roots: &mut RootSet) -> Result<ObjectId, EgoError> {
    int_arith(recv, args, arena, roots, "_IntAdd:", i64::checked_add, |a, b| a + b)
}

fn prim_int_sub(recv: ObjectId, args: &[ObjectId], arena: &mut Arena, roots: &mut RootSet) -> Result<ObjectId, EgoError> {
    int_arith(recv, args, arena, roots, "_IntSub:", i64::checked_sub, |a, b| a - b)
}

fn prim_int_mul(recv: ObjectId, args: &[ObjectId], arena: &mut Arena, roots: &mut RootSet) -> Result<ObjectId, EgoError> {
    int_arith(recv, args, arena, roots, "_IntMul:", i64::checked_mul, |a, b| a * b)
}

fn prim_int_div(recv: ObjectId, args: &[ObjectId], arena: &mut Arena, roots: &mut RootSet) -> Result<ObjectId, EgoError> {
    let a = as_int(recv, arena, "_IntDiv:")?;
    let arg = one_arg(args, "_IntDiv:")?;
    match as_num(arg, arena, "_IntDiv:")? {
        Num::Int(b) => {
            if b == 0 {
                return Err(div_zero_err());
            }
            a.checked_div(b).map(|q| make_int(q, arena, roots)).ok_or_else(overflow_err)
        }
        Num::Float(b) => {
            if b == 0.0 {
                return Err(div_zero_err());
            }
            Ok(make_float(a as f64 / b, arena, roots))
        }
    }
}

fn prim_int_lt(recv: ObjectId, args: &[ObjectId], arena: &mut Arena, roots: &mut RootSet) -> Result<ObjectId, EgoError> {
    int_cmp(recv, args, arena, roots, "_IntLt:", |a, b| a < b, |a, b| a < b)
}

fn prim_int_le(recv: ObjectId, args: &[ObjectId], arena: &mut Arena, roots: &mut RootSet) -> Result<ObjectId, EgoError> {
    int_cmp(recv, args, arena, roots, "_IntLe:", |a, b| a <= b, |a, b| a <= b)
}

fn prim_int_gt(recv: ObjectId, args: &[ObjectId], arena: &mut Arena, roots: &mut RootSet) -> Result<ObjectId, EgoError> {
    int_cmp(recv, args, arena, roots, "_IntGt:", |a, b| a > b, |a, b| a > b)
}

fn prim_int_ge(recv: ObjectId, args: &[ObjectId], arena: &mut Arena, roots: &mut RootSet) -> Result<ObjectId, EgoError> {
    int_cmp(recv, args, arena, roots, "_IntGe:", |a, b| a >= b, |a, b| a >= b)
}

fn prim_int_eq(recv: ObjectId, args: &[ObjectId], arena: &mut Arena, roots: &mut RootSet) -> Result<ObjectId, EgoError> {
    int_cmp(recv, args, arena, roots, "_IntEq:", |a, b| a == b, |a, b| a == b)
}

fn prim_int_ne(recv: ObjectId, args: &[ObjectId], arena: &mut Arena, roots: &mut RootSet) -> Result<ObjectId, EgoError> {
    int_cmp(recv, args, arena, roots, "_IntNe:", |a, b| a != b, |a, b| a != b)
}

fn prim_float_add(recv: ObjectId, args: &[ObjectId], arena: &mut Arena, roots: &mut RootSet) -> Result<ObjectId, EgoError> {
    float_arith(recv, args, arena, roots, "_FloatAdd:", |a, b| a + b)
}

fn prim_float_sub(recv: ObjectId, args: &[ObjectId], arena: &mut Arena, roots: &mut RootSet) -> Result<ObjectId, EgoError> {
    float_arith(recv, args, arena, roots, "_FloatSub:", |a, b| a - b)
}

fn prim_float_mul(recv: ObjectId, args: &[ObjectId], arena: &mut Arena, roots: &mut RootSet) -> Result<ObjectId, EgoError> {
    float_arith(recv, args, arena, roots, "_FloatMul:", |a, b| a * b)
}

fn prim_float_div(recv: ObjectId, args: &[ObjectId], arena: &mut Arena, roots: &mut RootSet) -> Result<ObjectId, EgoError> {
    let a = as_float(recv, arena, "_FloatDiv:")?;
    let arg = one_arg(args, "_FloatDiv:")?;
    let b = as_num(arg, arena, "_FloatDiv:")?.as_f64();
    if b == 0.0 {
        return Err(div_zero_err());
    }
    Ok(make_float(a / b, arena, roots))
}

fn prim_float_lt(recv: ObjectId, args: &[ObjectId], arena: &mut Arena, roots: &mut RootSet) -> Result<ObjectId, EgoError> {
    float_cmp(recv, args, arena, roots, "_FloatLt:", |a, b| a < b)
}

fn prim_float_le(recv: ObjectId, args: &[ObjectId], arena: &mut Arena, roots: &mut RootSet) -> Result<ObjectId, EgoError> {
    float_cmp(recv, args, arena, roots, "_FloatLe:", |a, b| a <= b)
}

fn prim_float_gt(recv: ObjectId, args: &[ObjectId], arena: &mut Arena, roots: &mut RootSet) -> Result<ObjectId, EgoError> {
    float_cmp(recv, args, arena, roots, "_FloatGt:", |a, b| a > b)
}

fn prim_float_ge(recv: ObjectId, args: &[ObjectId], arena: &mut Arena, roots: &mut RootSet) -> Result<ObjectId, EgoError> {
    float_cmp(recv, args, arena, roots, "_FloatGe:", |a, b| a >= b)
}

fn prim_float_eq(recv: ObjectId, args: &[ObjectId], arena: &mut Arena, roots: &mut RootSet) -> Result<ObjectId, EgoError> {
    float_cmp(recv, args, arena, roots, "_FloatEq:", |a, b| a == b)
}

fn prim_float_ne(recv: ObjectId, args: &[ObjectId], arena: &mut Arena, roots: &mut RootSet) -> Result<ObjectId, EgoError> {
    float_cmp(recv, args, arena, roots, "_FloatNe:", |a, b| a != b)
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

    prims.register("_IntAdd:", prim_int_add);
    prims.register("_IntSub:", prim_int_sub);
    prims.register("_IntMul:", prim_int_mul);
    prims.register("_IntDiv:", prim_int_div);
    prims.register("_IntLt:", prim_int_lt);
    prims.register("_IntLe:", prim_int_le);
    prims.register("_IntGt:", prim_int_gt);
    prims.register("_IntGe:", prim_int_ge);
    prims.register("_IntEq:", prim_int_eq);
    prims.register("_IntNe:", prim_int_ne);

    prims.register("_FloatAdd:", prim_float_add);
    prims.register("_FloatSub:", prim_float_sub);
    prims.register("_FloatMul:", prim_float_mul);
    prims.register("_FloatDiv:", prim_float_div);
    prims.register("_FloatLt:", prim_float_lt);
    prims.register("_FloatLe:", prim_float_le);
    prims.register("_FloatGt:", prim_float_gt);
    prims.register("_FloatGe:", prim_float_ge);
    prims.register("_FloatEq:", prim_float_eq);
    prims.register("_FloatNe:", prim_float_ne);
}
