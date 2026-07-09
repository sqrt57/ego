use std::collections::HashMap;
use std::rc::Rc;

use num_bigint::BigInt;
use num_traits::ToPrimitive;

use crate::arena::{Arena, ObjectId};
use crate::error::{EgoError, ErrorKind, SourceSpan};
use crate::gc::{alloc_with_gc, collect, make_string, RootSet};
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
    let s: Box<str> = match &arena.get(recv).kind {
        ObjectKind::Integer(n) => n.to_string().into_boxed_str(),
        ObjectKind::BigInt(n) => n.to_string().into_boxed_str(),
        _ => return Err(EgoError::with_kind(prim_span(), "_IntPrintString requires integer receiver".into(), ErrorKind::PrimitiveError)),
    };
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
        _ => return Err(EgoError::with_kind(prim_span(), "_FloatPrintString requires float receiver".into(), ErrorKind::PrimitiveError)),
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

fn prim_string_print_string(
    recv: ObjectId,
    _args: &[ObjectId],
    _arena: &mut Arena,
    _roots: &mut RootSet,
) -> Result<ObjectId, EgoError> {
    Ok(recv)
}

fn as_string<'a>(id: ObjectId, arena: &'a Arena, ctx: &str) -> Result<&'a str, EgoError> {
    match &arena.get(id).kind {
        ObjectKind::StringVal(s) => Ok(s),
        _ => Err(EgoError::with_kind(prim_span(), format!("{ctx} requires string receiver"), ErrorKind::PrimitiveError)),
    }
}


fn prim_string_concat(
    recv: ObjectId,
    args: &[ObjectId],
    arena: &mut Arena,
    roots: &mut RootSet,
) -> Result<ObjectId, EgoError> {
    let a = as_string(recv, arena, "_StringConcat:")?.to_string();
    let arg = one_arg(args, "_StringConcat:")?;
    let b = as_string(arg, arena, "_StringConcat:")?;
    Ok(make_string(format!("{a}{b}"), arena, roots))
}

// ── Numeric arithmetic and comparison ──────────────────────────────────────

enum Num {
    Int(i64),
    BigInt(BigInt),
    Float(f64),
}

impl Num {
    fn as_f64(&self) -> f64 {
        match self {
            Num::Int(n) => *n as f64,
            Num::BigInt(b) => b.to_f64().unwrap_or(f64::INFINITY),
            Num::Float(f) => *f,
        }
    }
}

/// Receiver-side view for integer arithmetic: a receiver is either a plain
/// `Integer` or an already-promoted `BigInt` — never a `Float` (arithmetic
/// selectors like `_IntAdd:` are only ever bound on `integer_proto`).
enum IntRepr {
    Small(i64),
    Big(BigInt),
}

impl IntRepr {
    fn to_big(self) -> BigInt {
        match self {
            IntRepr::Small(n) => BigInt::from(n),
            IntRepr::Big(n) => n,
        }
    }

    fn as_f64(&self) -> f64 {
        match self {
            IntRepr::Small(n) => *n as f64,
            IntRepr::Big(n) => n.to_f64().unwrap_or(f64::INFINITY),
        }
    }
}

fn as_int_repr(id: ObjectId, arena: &Arena, ctx: &str) -> Result<IntRepr, EgoError> {
    match &arena.get(id).kind {
        ObjectKind::Integer(n) => Ok(IntRepr::Small(*n)),
        ObjectKind::BigInt(n) => Ok(IntRepr::Big((**n).clone())),
        _ => Err(EgoError::with_kind(prim_span(), format!("{ctx} requires integer receiver"), ErrorKind::PrimitiveError)),
    }
}

fn as_int(id: ObjectId, arena: &Arena, ctx: &str) -> Result<i64, EgoError> {
    match arena.get(id).kind {
        ObjectKind::Integer(n) => Ok(n),
        _ => Err(EgoError::with_kind(prim_span(), format!("{ctx} requires integer receiver"), ErrorKind::PrimitiveError)),
    }
}

fn as_float(id: ObjectId, arena: &Arena, ctx: &str) -> Result<f64, EgoError> {
    match arena.get(id).kind {
        ObjectKind::Float(f) => Ok(f),
        _ => Err(EgoError::with_kind(prim_span(), format!("{ctx} requires float receiver"), ErrorKind::PrimitiveError)),
    }
}

fn as_num(id: ObjectId, arena: &Arena, ctx: &str) -> Result<Num, EgoError> {
    match &arena.get(id).kind {
        ObjectKind::Integer(n) => Ok(Num::Int(*n)),
        ObjectKind::BigInt(n) => Ok(Num::BigInt((**n).clone())),
        ObjectKind::Float(f) => Ok(Num::Float(*f)),
        _ => Err(EgoError::with_kind(prim_span(), format!("{ctx} requires a numeric argument"), ErrorKind::PrimitiveError)),
    }
}

fn one_arg(args: &[ObjectId], ctx: &str) -> Result<ObjectId, EgoError> {
    args.first().copied().ok_or_else(|| {
        EgoError::with_kind(prim_span(), format!("{ctx} requires one argument"), ErrorKind::PrimitiveError)
    })
}

fn div_zero_err() -> EgoError {
    EgoError::with_kind(prim_span(), "division by zero".into(), ErrorKind::ZeroDivide)
}

fn make_int(n: i64, arena: &mut Arena, roots: &RootSet) -> ObjectId {
    let id = alloc_with_gc(arena, roots, Object::new(ObjectKind::Integer(n)));
    arena.get_mut(id).slots.push(Slot {
        name: "parent".to_string(),
        kind: SlotKind::Parent,
        value: roots.integer_proto,
    });
    id
}

fn make_float(f: f64, arena: &mut Arena, roots: &RootSet) -> ObjectId {
    let id = alloc_with_gc(arena, roots, Object::new(ObjectKind::Float(f)));
    arena.get_mut(id).slots.push(Slot {
        name: "parent".to_string(),
        kind: SlotKind::Parent,
        value: roots.float_proto,
    });
    id
}

fn make_bigint(n: BigInt, arena: &mut Arena, roots: &RootSet) -> ObjectId {
    let id = alloc_with_gc(arena, roots, Object::new(ObjectKind::BigInt(Box::new(n))));
    arena.get_mut(id).slots.push(Slot {
        name: "parent".to_string(),
        kind: SlotKind::Parent,
        value: roots.integer_proto,
    });
    id
}

/// Picks the canonical representation for an integer arithmetic result: a
/// plain `Integer` when it fits in `i64`, otherwise a `BigInt`. Every
/// bignum-producing primitive routes its result through this so that a
/// value has exactly one representation regardless of how it was computed
/// (needed for `=`/`~=` to stay correct across `Integer`/`BigInt`).
fn make_norm_int(n: BigInt, arena: &mut Arena, roots: &RootSet) -> ObjectId {
    match n.to_i64() {
        Some(small) => make_int(small, arena, roots),
        None => make_bigint(n, arena, roots),
    }
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
    big_op: fn(&BigInt, &BigInt) -> BigInt,
    float_op: fn(f64, f64) -> f64,
) -> Result<ObjectId, EgoError> {
    let a = as_int_repr(recv, arena, sel)?;
    let arg = one_arg(args, sel)?;
    match as_num(arg, arena, sel)? {
        Num::Int(b) => match a {
            IntRepr::Small(a) => match int_op(a, b) {
                Some(s) => Ok(make_int(s, arena, roots)),
                None => Ok(make_norm_int(big_op(&BigInt::from(a), &BigInt::from(b)), arena, roots)),
            },
            IntRepr::Big(a) => Ok(make_norm_int(big_op(&a, &BigInt::from(b)), arena, roots)),
        },
        Num::BigInt(b) => Ok(make_norm_int(big_op(&a.to_big(), &b), arena, roots)),
        Num::Float(b) => Ok(make_float(float_op(a.as_f64(), b), arena, roots)),
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
    big_op: fn(&BigInt, &BigInt) -> bool,
    float_op: fn(f64, f64) -> bool,
) -> Result<ObjectId, EgoError> {
    let a = as_int_repr(recv, arena, sel)?;
    let arg = one_arg(args, sel)?;
    let result = match as_num(arg, arena, sel)? {
        Num::Int(b) => match a {
            IntRepr::Small(a) => int_op(a, b),
            IntRepr::Big(a) => big_op(&a, &BigInt::from(b)),
        },
        Num::BigInt(b) => big_op(&a.to_big(), &b),
        Num::Float(b) => float_op(a.as_f64(), b),
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
    int_arith(recv, args, arena, roots, "_IntAdd:", i64::checked_add, |a, b| a + b, |a, b| a + b)
}

fn prim_int_sub(recv: ObjectId, args: &[ObjectId], arena: &mut Arena, roots: &mut RootSet) -> Result<ObjectId, EgoError> {
    int_arith(recv, args, arena, roots, "_IntSub:", i64::checked_sub, |a, b| a - b, |a, b| a - b)
}

fn prim_int_mul(recv: ObjectId, args: &[ObjectId], arena: &mut Arena, roots: &mut RootSet) -> Result<ObjectId, EgoError> {
    int_arith(recv, args, arena, roots, "_IntMul:", i64::checked_mul, |a, b| a * b, |a, b| a * b)
}

fn prim_int_div(recv: ObjectId, args: &[ObjectId], arena: &mut Arena, roots: &mut RootSet) -> Result<ObjectId, EgoError> {
    let a = as_int_repr(recv, arena, "_IntDiv:")?;
    let arg = one_arg(args, "_IntDiv:")?;
    match as_num(arg, arena, "_IntDiv:")? {
        Num::Int(b) => {
            if b == 0 {
                return Err(div_zero_err());
            }
            match a {
                IntRepr::Small(a) => match a.checked_div(b) {
                    Some(q) => Ok(make_int(q, arena, roots)),
                    None => Ok(make_norm_int(BigInt::from(a) / BigInt::from(b), arena, roots)),
                },
                IntRepr::Big(a) => Ok(make_norm_int(a / BigInt::from(b), arena, roots)),
            }
        }
        Num::BigInt(b) => {
            if b == BigInt::from(0) {
                return Err(div_zero_err());
            }
            Ok(make_norm_int(a.to_big() / b, arena, roots))
        }
        Num::Float(b) => {
            if b == 0.0 {
                return Err(div_zero_err());
            }
            Ok(make_float(a.as_f64() / b, arena, roots))
        }
    }
}

fn prim_int_lt(recv: ObjectId, args: &[ObjectId], arena: &mut Arena, roots: &mut RootSet) -> Result<ObjectId, EgoError> {
    int_cmp(recv, args, arena, roots, "_IntLt:", |a, b| a < b, |a, b| a < b, |a, b| a < b)
}

fn prim_int_le(recv: ObjectId, args: &[ObjectId], arena: &mut Arena, roots: &mut RootSet) -> Result<ObjectId, EgoError> {
    int_cmp(recv, args, arena, roots, "_IntLe:", |a, b| a <= b, |a, b| a <= b, |a, b| a <= b)
}

fn prim_int_gt(recv: ObjectId, args: &[ObjectId], arena: &mut Arena, roots: &mut RootSet) -> Result<ObjectId, EgoError> {
    int_cmp(recv, args, arena, roots, "_IntGt:", |a, b| a > b, |a, b| a > b, |a, b| a > b)
}

fn prim_int_ge(recv: ObjectId, args: &[ObjectId], arena: &mut Arena, roots: &mut RootSet) -> Result<ObjectId, EgoError> {
    int_cmp(recv, args, arena, roots, "_IntGe:", |a, b| a >= b, |a, b| a >= b, |a, b| a >= b)
}

fn prim_int_eq(recv: ObjectId, args: &[ObjectId], arena: &mut Arena, roots: &mut RootSet) -> Result<ObjectId, EgoError> {
    int_cmp(recv, args, arena, roots, "_IntEq:", |a, b| a == b, |a, b| a == b, |a, b| a == b)
}

fn prim_int_ne(recv: ObjectId, args: &[ObjectId], arena: &mut Arena, roots: &mut RootSet) -> Result<ObjectId, EgoError> {
    int_cmp(recv, args, arena, roots, "_IntNe:", |a, b| a != b, |a, b| a != b, |a, b| a != b)
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
        return Err(EgoError::with_kind(prim_span(), "_PrintLine: requires an argument".into(), ErrorKind::PrimitiveError));
    }
    match &arena.get(args[0]).kind {
        ObjectKind::StringVal(s) => {
            println!("{s}");
            Ok(roots.nil_id)
        }
        _ => Err(EgoError::with_kind(prim_span(), "_PrintLine: requires a string argument".into(), ErrorKind::PrimitiveError)),
    }
}

// ── Arrays ──────────────────────────────────────────────────────────────────

fn array_elems<'a>(id: ObjectId, arena: &'a Arena, ctx: &str) -> Result<&'a [ObjectId], EgoError> {
    match &arena.get(id).kind {
        ObjectKind::Array(v) => Ok(v),
        _ => Err(EgoError::with_kind(prim_span(), format!("{ctx} requires array receiver"), ErrorKind::PrimitiveError)),
    }
}

/// Validates a 1-based array index, returning the corresponding 0-based
/// offset. Out-of-range and non-integer indices both signal `primitiveError`
/// (mirrors `_StringConcat:`'s non-string-argument convention — no separate
/// "index out of range" exception type exists yet).
fn as_index(id: ObjectId, len: usize, arena: &Arena, ctx: &str) -> Result<usize, EgoError> {
    let n = as_int(id, arena, ctx)?;
    if n < 1 || (n as usize) > len {
        return Err(EgoError::with_kind(prim_span(), format!("{ctx}: index {n} out of range (1..{len})"), ErrorKind::PrimitiveError));
    }
    Ok((n - 1) as usize)
}

fn prim_array_new(
    recv: ObjectId,
    args: &[ObjectId],
    arena: &mut Arena,
    roots: &mut RootSet,
) -> Result<ObjectId, EgoError> {
    let _ = recv;
    let arg = one_arg(args, "_ArrayNew:")?;
    let n = as_int(arg, arena, "_ArrayNew:")?;
    if n < 0 {
        return Err(EgoError::with_kind(prim_span(), "_ArrayNew: requires a non-negative size".into(), ErrorKind::PrimitiveError));
    }
    let elems = vec![roots.nil_id; n as usize];
    let id = alloc_with_gc(arena, roots, Object::new(ObjectKind::Array(elems)));
    arena.get_mut(id).slots.push(Slot {
        name: "parent".to_string(),
        kind: SlotKind::Parent,
        value: roots.array_proto,
    });
    Ok(id)
}

fn prim_array_size(
    recv: ObjectId,
    _args: &[ObjectId],
    arena: &mut Arena,
    roots: &mut RootSet,
) -> Result<ObjectId, EgoError> {
    let len = array_elems(recv, arena, "_ArraySize")?.len();
    Ok(make_int(len as i64, arena, roots))
}

fn prim_array_at(
    recv: ObjectId,
    args: &[ObjectId],
    arena: &mut Arena,
    _roots: &mut RootSet,
) -> Result<ObjectId, EgoError> {
    let arg = one_arg(args, "_ArrayAt:")?;
    let len = array_elems(recv, arena, "_ArrayAt:")?.len();
    let idx = as_index(arg, len, arena, "_ArrayAt:")?;
    Ok(array_elems(recv, arena, "_ArrayAt:")?[idx])
}

fn prim_array_at_put(
    recv: ObjectId,
    args: &[ObjectId],
    arena: &mut Arena,
    _roots: &mut RootSet,
) -> Result<ObjectId, EgoError> {
    if args.len() < 2 {
        return Err(EgoError::with_kind(prim_span(), "_ArrayAt:Put: requires two arguments".into(), ErrorKind::PrimitiveError));
    }
    let (idx_arg, val) = (args[0], args[1]);
    let len = array_elems(recv, arena, "_ArrayAt:Put:")?.len();
    let idx = as_index(idx_arg, len, arena, "_ArrayAt:Put:")?;
    match &mut arena.get_mut(recv).kind {
        ObjectKind::Array(v) => v[idx] = val,
        _ => unreachable!("array_elems already validated recv is an Array"),
    }
    Ok(val)
}

/// Renders array elements without message dispatch — a bare `PrimFn` only
/// has `Arena`/`RootSet`, not the full `Interpreter` needed to send
/// `printString` to an arbitrary element (same limitation block `value`
/// hits, see eval.rs). Known value kinds print their real content; anything
/// else (plain user objects, methods, blocks) falls back to a placeholder.
fn render_element(id: ObjectId, arena: &Arena, roots: &RootSet) -> String {
    if id == roots.nil_id {
        return "nil".to_string();
    }
    if id == roots.true_id {
        return "true".to_string();
    }
    if id == roots.false_id {
        return "false".to_string();
    }
    match &arena.get(id).kind {
        ObjectKind::Integer(n) => n.to_string(),
        ObjectKind::BigInt(n) => n.to_string(),
        ObjectKind::Float(f) => format_float(*f),
        ObjectKind::StringVal(s) => s.to_string(),
        ObjectKind::Array(elems) => {
            let parts: Vec<String> = elems.iter().map(|&e| render_element(e, arena, roots)).collect();
            format!("({})", parts.join(" "))
        }
        _ => "<object>".to_string(),
    }
}

fn prim_array_print_string(
    recv: ObjectId,
    _args: &[ObjectId],
    arena: &mut Arena,
    roots: &mut RootSet,
) -> Result<ObjectId, EgoError> {
    let elems = array_elems(recv, arena, "_ArrayPrintString")?.to_vec();
    let parts: Vec<String> = elems.iter().map(|&e| render_element(e, arena, roots)).collect();
    Ok(make_string(format!("({})", parts.join(" ")), arena, roots))
}

// ── Mirrors ─────────────────────────────────────────────────────────────────

fn mirror_reflectee(id: ObjectId, arena: &Arena, ctx: &str) -> Result<ObjectId, EgoError> {
    match arena.get(id).kind {
        ObjectKind::Mirror(reflectee) => Ok(reflectee),
        _ => Err(EgoError::with_kind(prim_span(), format!("{ctx} requires mirror receiver"), ErrorKind::PrimitiveError)),
    }
}

fn make_array(elems: Vec<ObjectId>, arena: &mut Arena, roots: &RootSet) -> ObjectId {
    let id = alloc_with_gc(arena, roots, Object::new(ObjectKind::Array(elems)));
    arena.get_mut(id).slots.push(Slot {
        name: "parent".to_string(),
        kind: SlotKind::Parent,
        value: roots.array_proto,
    });
    id
}

fn find_slot_index(id: ObjectId, name: &str, arena: &Arena) -> Option<usize> {
    arena.get(id).slots.iter().position(|s| s.name == name)
}

fn no_such_slot_err(ctx: &str, name: &str) -> EgoError {
    EgoError::with_kind(prim_span(), format!("{ctx}: no slot named '{name}'"), ErrorKind::PrimitiveError)
}

fn prim_mirror_of(
    _recv: ObjectId,
    args: &[ObjectId],
    arena: &mut Arena,
    roots: &mut RootSet,
) -> Result<ObjectId, EgoError> {
    let target = one_arg(args, "_MirrorOf:")?;
    let id = alloc_with_gc(arena, roots, Object::new(ObjectKind::Mirror(target)));
    arena.get_mut(id).slots.push(Slot {
        name: "parent".to_string(),
        kind: SlotKind::Parent,
        value: roots.mirror_proto,
    });
    Ok(id)
}

fn prim_mirror_slot_names(
    recv: ObjectId,
    _args: &[ObjectId],
    arena: &mut Arena,
    roots: &mut RootSet,
) -> Result<ObjectId, EgoError> {
    let reflectee = mirror_reflectee(recv, arena, "_MirrorSlotNames")?;
    let names: Vec<String> = arena.get(reflectee).slots.iter().map(|s| s.name.clone()).collect();
    let elems: Vec<ObjectId> = names.into_iter().map(|n| make_string(n, arena, roots)).collect();
    Ok(make_array(elems, arena, roots))
}

fn prim_mirror_at(
    recv: ObjectId,
    args: &[ObjectId],
    arena: &mut Arena,
    _roots: &mut RootSet,
) -> Result<ObjectId, EgoError> {
    let arg = one_arg(args, "_MirrorAt:")?;
    let reflectee = mirror_reflectee(recv, arena, "_MirrorAt:")?;
    let name = as_string(arg, arena, "_MirrorAt:")?.to_string();
    let idx = find_slot_index(reflectee, &name, arena).ok_or_else(|| no_such_slot_err("_MirrorAt:", &name))?;
    Ok(arena.get(reflectee).slots[idx].value)
}

fn prim_mirror_at_put(
    recv: ObjectId,
    args: &[ObjectId],
    arena: &mut Arena,
    _roots: &mut RootSet,
) -> Result<ObjectId, EgoError> {
    if args.len() < 2 {
        return Err(EgoError::with_kind(prim_span(), "_MirrorAt:Put: requires two arguments".into(), ErrorKind::PrimitiveError));
    }
    let (name_arg, val) = (args[0], args[1]);
    let reflectee = mirror_reflectee(recv, arena, "_MirrorAt:Put:")?;
    let name = as_string(name_arg, arena, "_MirrorAt:Put:")?.to_string();
    let idx = find_slot_index(reflectee, &name, arena).ok_or_else(|| no_such_slot_err("_MirrorAt:Put:", &name))?;
    arena.get_mut(reflectee).slots[idx].value = val;
    Ok(val)
}

fn prim_mirror_add_slot(
    recv: ObjectId,
    args: &[ObjectId],
    arena: &mut Arena,
    _roots: &mut RootSet,
) -> Result<ObjectId, EgoError> {
    if args.len() < 2 {
        return Err(EgoError::with_kind(prim_span(), "_MirrorAddSlot:Value: requires two arguments".into(), ErrorKind::PrimitiveError));
    }
    let (name_arg, val) = (args[0], args[1]);
    let reflectee = mirror_reflectee(recv, arena, "_MirrorAddSlot:Value:")?;
    let name = as_string(name_arg, arena, "_MirrorAddSlot:Value:")?.to_string();
    arena.get_mut(reflectee).slots.push(Slot { name, kind: SlotKind::Data, value: val });
    Ok(val)
}

fn prim_mirror_remove_slot(
    recv: ObjectId,
    args: &[ObjectId],
    arena: &mut Arena,
    roots: &mut RootSet,
) -> Result<ObjectId, EgoError> {
    let arg = one_arg(args, "_MirrorRemoveSlot:")?;
    let reflectee = mirror_reflectee(recv, arena, "_MirrorRemoveSlot:")?;
    let name = as_string(arg, arena, "_MirrorRemoveSlot:")?.to_string();
    let idx = find_slot_index(reflectee, &name, arena).ok_or_else(|| no_such_slot_err("_MirrorRemoveSlot:", &name))?;
    arena.get_mut(reflectee).slots.remove(idx);
    Ok(roots.nil_id)
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
    prims.register("_StringPrintString", prim_string_print_string);
    prims.register("_StringConcat:", prim_string_concat);
    prims.register("_PrintLine:", prim_print_line);
    prims.register("_GcCollect", prim_gc_collect);

    prims.register("_ArrayNew:", prim_array_new);
    prims.register("_ArraySize", prim_array_size);
    prims.register("_ArrayAt:", prim_array_at);
    prims.register("_ArrayAt:Put:", prim_array_at_put);
    prims.register("_ArrayPrintString", prim_array_print_string);

    prims.register("_MirrorOf:", prim_mirror_of);
    prims.register("_MirrorSlotNames", prim_mirror_slot_names);
    prims.register("_MirrorAt:", prim_mirror_at);
    prims.register("_MirrorAt:Put:", prim_mirror_at_put);
    prims.register("_MirrorAddSlot:Value:", prim_mirror_add_slot);
    prims.register("_MirrorRemoveSlot:", prim_mirror_remove_slot);

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
