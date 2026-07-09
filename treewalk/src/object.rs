use std::rc::Rc;

use crate::arena::ObjectId;
use crate::ast::{BlockLit, Stmt};
use crate::env::{ActivationId, Env};
use crate::error::SourceSpan;

#[derive(Debug)]
pub struct Object {
    pub mark: bool,
    pub kind: ObjectKind,
    pub slots: Vec<Slot>,
}

impl Object {
    pub fn new(kind: ObjectKind) -> Self {
        Self { mark: false, kind, slots: Vec::new() }
    }
}

#[derive(Debug)]
pub enum ObjectKind {
    Plain,
    Integer(i64),
    Float(f64),
    StringVal(Box<str>),
    Method(Rc<MethodDef>),
    Block(Box<BlockData>),
    /// Synthetic setter installed alongside every `Var` slot; holds the
    /// name of the var slot it mutates. Not backed by AST or the primitive
    /// table since primitives are bare fn pointers with no per-slot state.
    VarSetter(String),
    /// Fixed-size indexed sequence (substage 1.16). No byte/object-vector
    /// distinction, unlike Self's VM-level array.
    Array(Vec<ObjectId>),
}

#[derive(Debug)]
pub struct Slot {
    pub name: String,
    pub kind: SlotKind,
    pub value: ObjectId,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SlotKind {
    Data,
    Var,
    Arg,
    Method,
    Parent,
}

#[derive(Debug)]
pub struct MethodDef {
    pub params: Vec<String>,
    pub body: Vec<Stmt>,
    pub source: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct BlockData {
    pub lit: Rc<BlockLit>,
    pub home_id: ActivationId,
    pub captured_self: ObjectId,
    pub captured_resend: Option<ObjectId>,
    pub captures: Env,
}
