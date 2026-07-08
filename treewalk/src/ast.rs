use std::rc::Rc;

use crate::error::SourceSpan;

// ── Program ────────────────────────────────────────────────────────────────

pub type Program = Vec<Stmt>;

// ── Statements ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StmtKind {
    Return(Box<Expr>),
    Expr(Box<Expr>),
}

// ── Expressions ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    // Literals
    Int(i64),
    Float(f64),
    Str(String),

    // Variables
    Ident(String),
    Self_,
    // `name <- expr` outside a slot-decl header: binds/rebinds `name` in the
    // enclosing activation's env directly, bypassing message dispatch. Only
    // recognized when the LHS is a bare identifier (see parser.rs).
    Assign { name: String, value: Box<Expr> },

    // Message sends
    UnarySend  { recv: Box<Expr>, sel: String },
    BinarySend { recv: Box<Expr>, sel: String, arg: Box<Expr> },
    // `sel` is the full assembled selector, e.g. "at:Put:"; `args` in part order.
    KeywordSend { recv: Box<Expr>, sel: String, args: Vec<Expr> },
    // `resend.sel` (Undirected) or `parentName.sel` (Directed) — `sel`/`args`
    // follow the same convention as `KeywordSend` for the keyword case.
    ResendSend { target: ResendTarget, sel: String, args: Vec<Expr> },

    // Cascade: one receiver, one or more continuation messages
    Cascade { recv: Box<Expr>, msgs: Vec<CascadeMsg> },

    // Compound literals
    Block(Rc<BlockLit>),
    Object(Box<ObjectLit>),
}

// ── Resend targets ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum ResendTarget {
    /// `resend.sel` — continue lookup from the parent chain of the object
    /// that defined the currently executing method.
    Undirected,
    /// `name.sel` — continue lookup from one specific parent slot (named
    /// `name`) of the object that defined the currently executing method.
    Directed(String),
}

// ── Cascade messages ───────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum CascadeMsg {
    Unary   { sel: String,                   span: SourceSpan },
    Binary  { sel: String, arg: Expr,        span: SourceSpan },
    Keyword { sel: String, args: Vec<Expr>,  span: SourceSpan },
}

// ── Block literals ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct BlockLit {
    pub params: Vec<String>,
    pub locals: Vec<BlockLocal>,
    pub body:   Vec<Stmt>,
    pub span:   SourceSpan,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BlockLocal {
    pub name: String,
    pub kind: LocalKind,
    pub init: Expr,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LocalKind { Data, Var }

// ── Object literals ────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct ObjectLit {
    pub annotation: Option<String>,
    pub slots:      Vec<SlotDecl>,
    pub body:       Vec<Stmt>,
    pub span:       SourceSpan,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SlotDecl {
    pub kind: SlotDeclKind,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SlotDeclKind {
    Data   { name: String, value: Expr },
    Var    { name: String, value: Expr },
    Arg    { name: String },
    Parent { name: String, value: Expr },
    Method { sel: MethodSel, body: Vec<Stmt> },
}

#[derive(Debug, Clone, PartialEq)]
pub enum MethodSel {
    Unary(String),
    Binary(String, String),         // selector, param name
    Keyword(Vec<(String, String)>), // (keyword_part, param_name) pairs
}

impl MethodSel {
    /// Full assembled selector string, e.g. "at:Put:" for a keyword method.
    pub fn selector(&self) -> String {
        match self {
            MethodSel::Unary(s)   => s.clone(),
            MethodSel::Binary(s, _) => s.clone(),
            MethodSel::Keyword(parts) => parts.iter().map(|(kw, _)| kw.as_str()).collect(),
        }
    }

    pub fn params(&self) -> Vec<String> {
        match self {
            MethodSel::Unary(_)         => vec![],
            MethodSel::Binary(_, p)     => vec![p.clone()],
            MethodSel::Keyword(parts)   => parts.iter().map(|(_, p)| p.clone()).collect(),
        }
    }
}

// ── Shared reference wrapper for method/block bodies ───────────────────────
//
// The evaluator clones method bodies into ObjectKind::Method without copying
// the AST; Rc<...> here matches the MethodDef design in rs-treewalk-impl.md.

pub type Body = Rc<Vec<Stmt>>;

pub fn body(stmts: Vec<Stmt>) -> Body {
    Rc::new(stmts)
}
