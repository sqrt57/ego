use std::rc::Rc;

use crate::arena::ObjectId;
use crate::ast::{
    CascadeMsg, Expr, ExprKind, ObjectLit, Program, ResendTarget, SlotDeclKind, Stmt, StmtKind,
};
use crate::bootstrap::Interpreter;
use crate::env::{ActivationId, Env, env_new};
use crate::error::{EgoError, SourceSpan};
use crate::gc::alloc_with_gc;
use crate::lexer::lex;
use crate::object::{BlockData, MethodDef, Object, ObjectKind, Slot, SlotKind};
use crate::parser::parse;

pub enum EgoSignal {
    Err(EgoError),
    Exception(ObjectId),
    NonLocalReturn(ActivationId, ObjectId),
}

pub type EvalResult = Result<ObjectId, EgoSignal>;

pub struct Activation {
    pub id: ActivationId,
    pub self_obj: ObjectId,
    pub resend_start: Option<ObjectId>,
    pub env: Env,
}

enum SlotLookup {
    /// The object whose own slot list holds the method, plus the method
    /// itself. The owner seeds `resend_start` for the activation this
    /// method runs in.
    Method(ObjectId, Rc<MethodDef>),
    Value(ObjectId),
    VarSetter(ObjectId, String),
}

/// Ordinary top-down lookup: `id`'s own slots first, then its parents.
fn lookup_slot(recv: ObjectId, sel: &str, interp: &Interpreter) -> Result<Option<SlotLookup>, String> {
    lookup_in(recv, sel, interp, &mut Vec::new())
}

fn lookup_in(
    id: ObjectId,
    sel: &str,
    interp: &Interpreter,
    visited: &mut Vec<ObjectId>,
) -> Result<Option<SlotLookup>, String> {
    // Cycle guard: bails out only for an object already on *this* downward
    // path (`visited` is pushed/popped, not accumulated across siblings) —
    // see `lookup_in_parents` for why siblings must be free to revisit the
    // same object, which is what makes diamond ambiguity detectable.
    if visited.contains(&id) {
        return Ok(None);
    }
    visited.push(id);

    let obj = interp.arena.get(id);
    for slot in &obj.slots {
        if slot.name == sel {
            let result = match &slot.kind {
                SlotKind::Method => match &interp.arena.get(slot.value).kind {
                    ObjectKind::Method(m) => Some(SlotLookup::Method(id, m.clone())),
                    ObjectKind::VarSetter(name) => Some(SlotLookup::VarSetter(id, name.clone())),
                    _ => None,
                },
                SlotKind::Data | SlotKind::Var => Some(SlotLookup::Value(slot.value)),
                // A parent slot is also an ordinary accessor: sending its
                // name returns the parent object itself.
                SlotKind::Parent => Some(SlotLookup::Value(slot.value)),
                SlotKind::Arg => None,
            };
            if result.is_some() {
                visited.pop();
                return Ok(result);
            }
        }
    }

    let parents: Vec<ObjectId> = obj
        .slots
        .iter()
        .filter(|s| s.kind == SlotKind::Parent)
        .map(|s| s.value)
        .collect();

    let result = lookup_in_parents(&parents, sel, interp, visited);
    visited.pop();
    result
}

/// Searches every parent in `parents` (depth-first, left to right) and
/// signals ambiguity if more than one yields a result — see self-notes.md §4.
fn lookup_in_parents(
    parents: &[ObjectId],
    sel: &str,
    interp: &Interpreter,
    visited: &mut Vec<ObjectId>,
) -> Result<Option<SlotLookup>, String> {
    let mut found: Option<SlotLookup> = None;
    for &parent_id in parents {
        if let Some(result) = lookup_in(parent_id, sel, interp, visited)? {
            if found.is_some() {
                return Err(format!(
                    "message not understood: {sel} is ambiguous (reachable through multiple parents)"
                ));
            }
            found = Some(result);
        }
    }
    Ok(found)
}

pub fn eval_send(
    recv: ObjectId,
    sel: &str,
    args: &[ObjectId],
    span: &SourceSpan,
    interp: &mut Interpreter,
) -> EvalResult {
    if sel.starts_with('_') {
        // Block activation (`_BlockValue`, `_BlockValue:`, `_BlockValue:Value:`)
        // needs to recursively run AST via the evaluator, which a `PrimFn`
        // cannot do — its signature only threads `Arena`/`RootSet`, not the
        // full `Interpreter`. Intercept these selectors here instead of
        // routing them through the primitive table.
        if is_block_value_selector(sel) && matches!(&interp.arena.get(recv).kind, ObjectKind::Block(_)) {
            let roots_base = interp.roots.stack_roots.len();
            interp.roots.stack_roots.push(recv);
            for &arg in args {
                interp.roots.stack_roots.push(arg);
            }
            let result = eval_block_call(recv, args, span, interp);
            interp.roots.stack_roots.truncate(roots_base);
            return result;
        }

        // `whileTrue:` and the boolean control-flow selectors all need to
        // send `value` to block arguments, which (like block activation
        // above) requires recursing into the evaluator rather than a bare
        // `PrimFn`. Intercepted here for the same reason.
        if sel == "_BlockWhileTrue:" {
            let roots_base = interp.roots.stack_roots.len();
            interp.roots.stack_roots.push(recv);
            for &arg in args {
                interp.roots.stack_roots.push(arg);
            }
            let result = eval_block_while_true(recv, args[0], span, interp);
            interp.roots.stack_roots.truncate(roots_base);
            return result;
        }

        if is_bool_control_selector(sel) {
            let roots_base = interp.roots.stack_roots.len();
            interp.roots.stack_roots.push(recv);
            for &arg in args {
                interp.roots.stack_roots.push(arg);
            }
            let result = eval_bool_control(recv, sel, args, span, interp);
            interp.roots.stack_roots.truncate(roots_base);
            return result;
        }

        let prim_fn = match interp.prims.get(sel) {
            Some(f) => f,
            None => {
                return Err(EgoSignal::Err(EgoError::new(
                    span.clone(),
                    format!("unknown primitive: {sel}"),
                )));
            }
        };
        // Protect recv and args across the primitive call, which may alloc.
        let roots_base = interp.roots.stack_roots.len();
        interp.roots.stack_roots.push(recv);
        for &arg in args {
            interp.roots.stack_roots.push(arg);
        }
        // Primitives have no access to source spans (`PrimFn` only threads
        // `Arena`/`RootSet`), so any error they raise carries a placeholder
        // `<primitive>:0:0` span (see `prim_span()` in primitives.rs). Stamp
        // it with the real call-site span here, the one place that has it.
        let result = prim_fn(recv, args, &mut interp.arena, &mut interp.roots)
            .map_err(|mut e| { e.span = span.clone(); EgoSignal::Err(e) });
        interp.roots.stack_roots.truncate(roots_base);
        return result;
    }

    let lookup = lookup_slot(recv, sel, interp);
    invoke_lookup(lookup, recv, sel, args, span, interp)
}

/// Sends `sel` starting the lookup from the parent chain of the object that
/// defined the currently executing method (`activation.resend_start`), per
/// the resend syntax — undirected (`resend.sel`) searches all of that
/// object's parents; directed (`name.sel`) is constrained to the one parent
/// slot named `name` on that object. `self` is unchanged: it stays the
/// original receiver throughout, never the resend target.
fn eval_resend(
    target: &ResendTarget,
    sel: &str,
    args: &[ObjectId],
    activation: &Activation,
    span: &SourceSpan,
    interp: &mut Interpreter,
) -> EvalResult {
    let owner = match activation.resend_start {
        Some(owner) => owner,
        None => {
            return Err(EgoSignal::Err(EgoError::new(
                span.clone(),
                "resend used outside a method".into(),
            )));
        }
    };

    let lookup = match target {
        ResendTarget::Undirected => {
            let parents: Vec<ObjectId> = interp
                .arena
                .get(owner)
                .slots
                .iter()
                .filter(|s| s.kind == SlotKind::Parent)
                .map(|s| s.value)
                .collect();
            lookup_in_parents(&parents, sel, interp, &mut Vec::new())
        }
        ResendTarget::Directed(name) => {
            let parent_val = interp
                .arena
                .get(owner)
                .slots
                .iter()
                .find(|s| s.kind == SlotKind::Parent && &s.name == name)
                .map(|s| s.value);
            match parent_val {
                Some(v) => lookup_in(v, sel, interp, &mut Vec::new()),
                None => {
                    return Err(EgoSignal::Err(EgoError::new(
                        span.clone(),
                        format!("directed resend: no parent slot named '{name}'"),
                    )));
                }
            }
        }
    };

    invoke_lookup(lookup, activation.self_obj, sel, args, span, interp)
}

/// Shared tail of `eval_send`/`eval_resend`: turns a completed slot lookup
/// into an evaluation result. `self_obj` is the activation the invoked
/// method should see as `self` — the message receiver for `eval_send`, the
/// *original* receiver (unchanged) for `eval_resend`.
fn invoke_lookup(
    lookup: Result<Option<SlotLookup>, String>,
    self_obj: ObjectId,
    sel: &str,
    args: &[ObjectId],
    span: &SourceSpan,
    interp: &mut Interpreter,
) -> EvalResult {
    match lookup {
        Ok(Some(SlotLookup::Method(owner, method_def))) => {
            eval_method(self_obj, Some(owner), method_def, args, span, interp)
        }
        Ok(Some(SlotLookup::Value(val))) => Ok(val),
        Ok(Some(SlotLookup::VarSetter(owner, name))) => {
            if args.len() != 1 {
                return Err(EgoSignal::Err(EgoError::new(
                    span.clone(),
                    format!(
                        "wrong number of arguments: expected 1, got {}",
                        args.len()
                    ),
                )));
            }
            let new_val = args[0];
            let slot = interp
                .arena
                .get_mut(owner)
                .slots
                .iter_mut()
                .find(|s| s.kind == SlotKind::Var && s.name == name);
            match slot {
                Some(s) => {
                    s.value = new_val;
                    Ok(new_val)
                }
                None => Err(EgoSignal::Err(EgoError::new(
                    span.clone(),
                    format!("var slot not found: {name}"),
                ))),
            }
        }
        Ok(None) => Err(EgoSignal::Err(EgoError::new(
            span.clone(),
            format!("message not understood: {sel}"),
        ))),
        Err(msg) => Err(EgoSignal::Err(EgoError::new(span.clone(), msg))),
    }
}

fn eval_method(
    self_obj: ObjectId,
    resend_start: Option<ObjectId>,
    method_def: Rc<MethodDef>,
    args: &[ObjectId],
    span: &SourceSpan,
    interp: &mut Interpreter,
) -> EvalResult {
    if args.len() != method_def.params.len() {
        return Err(EgoSignal::Err(EgoError::new(
            span.clone(),
            format!(
                "wrong number of arguments: expected {}, got {}",
                method_def.params.len(),
                args.len()
            ),
        )));
    }

    let id = interp.next_activation_id();
    let env = env_new();
    for (param, &arg) in method_def.params.iter().zip(args.iter()) {
        env.borrow_mut().insert(param.clone(), arg);
    }

    let activation = Activation { id, self_obj, resend_start, env };
    interp.live_activations.insert(id);

    // Protect self_obj, args, and env bindings across the method body, which may alloc.
    let roots_base = interp.roots.stack_roots.len();
    interp.roots.stack_roots.push(self_obj);
    for &arg in args {
        interp.roots.stack_roots.push(arg);
    }
    interp.roots.activation_envs.push(activation.env.clone());
    let result = eval_body(&method_def.body, &activation, interp);
    interp.roots.activation_envs.pop();
    interp.roots.stack_roots.truncate(roots_base);

    interp.live_activations.remove(&id);

    match result {
        Ok(v) => Ok(v),
        Err(EgoSignal::NonLocalReturn(target, val)) if target == id => Ok(val),
        Err(EgoSignal::NonLocalReturn(target, val)) => {
            if !interp.live_activations.contains(&target) {
                Err(EgoSignal::Err(EgoError::new(
                    method_def.source.clone(),
                    "block returned to a dead activation".into(),
                )))
            } else {
                Err(EgoSignal::NonLocalReturn(target, val))
            }
        }
        // Bootstrap-synthesized methods (`+`, `/`, etc. — see
        // `make_unary_prim_method`/`make_binary_prim_method` in bootstrap.rs)
        // have no real source text, so their body carries a placeholder
        // `<bootstrap>:0:0` span. An error raised there is only meaningful
        // at the real call site, which is `span` here — unlike a genuine
        // user-defined method body, whose own per-statement spans should be
        // kept as-is (an error inside the user's method should point inside
        // it, not at the call site).
        Err(EgoSignal::Err(mut e)) if *e.span.file == "<bootstrap>" => {
            e.span = span.clone();
            Err(EgoSignal::Err(e))
        }
        Err(e) => Err(e),
    }
}

fn is_block_value_selector(sel: &str) -> bool {
    matches!(sel, "_BlockValue" | "_BlockValue:" | "_BlockValue:Value:")
}

/// Repeatedly sends `value` to the condition block (`recv`) and, while it
/// answers `true`, to the body block (`body`) — the ordinary keyword method
/// `whileTrue:` on blocks (lang-spec.md §7). Answers `nil` once the
/// condition answers `false`; anything else is a fatal error, since ego has
/// no separate `Boolean` type to coerce against.
fn eval_block_while_true(recv: ObjectId, body: ObjectId, span: &SourceSpan, interp: &mut Interpreter) -> EvalResult {
    loop {
        let cond = eval_send(recv, "value", &[], span, interp)?;
        if cond == interp.roots.true_id {
            eval_send(body, "value", &[], span, interp)?;
        } else if cond == interp.roots.false_id {
            return Ok(interp.roots.nil_id);
        } else {
            return Err(EgoSignal::Err(EgoError::new(
                span.clone(),
                "whileTrue: condition block must evaluate to true or false".into(),
            )));
        }
    }
}

fn is_bool_control_selector(sel: &str) -> bool {
    matches!(
        sel,
        "_BoolIfTrue:False:" | "_BoolIfTrue:" | "_BoolIfFalse:" | "_BoolAnd:" | "_BoolOr:" | "_BoolNot"
    )
}

/// Backs `ifTrue:False:`, `ifTrue:`, `ifFalse:`, `and:`, `or:`, and `not` on
/// the `true`/`false` prototypes (lang-spec.md §7-8). Branches on `recv`'s
/// identity (there's no separate `Boolean` tag — `true`/`false` are the only
/// two instances) and sends `value` to whichever block argument was chosen;
/// the untaken branch's block is never invoked, giving `ifTrue:False:` its
/// required lazy-evaluation semantics.
fn eval_bool_control(recv: ObjectId, sel: &str, args: &[ObjectId], span: &SourceSpan, interp: &mut Interpreter) -> EvalResult {
    let is_true = recv == interp.roots.true_id;
    match sel {
        "_BoolIfTrue:False:" => {
            let branch = if is_true { args[0] } else { args[1] };
            eval_send(branch, "value", &[], span, interp)
        }
        "_BoolIfTrue:" => {
            if is_true {
                eval_send(args[0], "value", &[], span, interp)
            } else {
                Ok(interp.roots.nil_id)
            }
        }
        "_BoolIfFalse:" => {
            if is_true {
                Ok(interp.roots.nil_id)
            } else {
                eval_send(args[0], "value", &[], span, interp)
            }
        }
        "_BoolAnd:" => {
            if is_true {
                eval_send(args[0], "value", &[], span, interp)
            } else {
                Ok(recv)
            }
        }
        "_BoolOr:" => {
            if is_true {
                Ok(recv)
            } else {
                eval_send(args[0], "value", &[], span, interp)
            }
        }
        "_BoolNot" => Ok(if is_true { interp.roots.false_id } else { interp.roots.true_id }),
        _ => unreachable!("is_bool_control_selector gates this match"),
    }
}

/// Activates a block: binds `args` to the block's own param names and
/// evaluates its locals' initializers, both directly into `captures` (the
/// shared env from the point the block literal was evaluated — see
/// `object::BlockData`), then runs the body with `self`/`resend_start`
/// restored from capture and `activation.id` set to `home_id` so that `^`
/// raises `NonLocalReturn(home_id, _)`, targeting the enclosing method's
/// activation rather than this call.
///
/// Unlike `eval_method`, this never converts a matching `NonLocalReturn` to
/// `Ok` — that catch belongs solely to the `eval_method` frame whose own id
/// equals `home_id`. It only guards against a `^` whose target has already
/// exited (a dead block, `badBlockActivation` in lang-spec.md's error
/// table), checked lazily here rather than eagerly on every `value` send:
/// ego's `Env` is heap-allocated and GC-tracked, so invoking a block whose
/// home method already returned is safe as long as it never actually
/// executes `^`.
fn eval_block_call(block_id: ObjectId, args: &[ObjectId], span: &SourceSpan, interp: &mut Interpreter) -> EvalResult {
    let block: BlockData = match &interp.arena.get(block_id).kind {
        ObjectKind::Block(b) => (**b).clone(),
        _ => return Err(EgoSignal::Err(EgoError::new(span.clone(), "not a block".into()))),
    };

    if args.len() != block.lit.params.len() {
        return Err(EgoSignal::Err(EgoError::new(
            span.clone(),
            format!(
                "wrong number of arguments: expected {}, got {}",
                block.lit.params.len(),
                args.len()
            ),
        )));
    }

    for (param, &arg) in block.lit.params.iter().zip(args.iter()) {
        block.captures.borrow_mut().insert(param.clone(), arg);
    }

    let activation = Activation {
        id: block.home_id,
        self_obj: block.captured_self,
        resend_start: block.captured_resend,
        env: block.captures.clone(),
    };

    for local in &block.lit.locals {
        let val = eval_expr(&local.init, &activation, interp)?;
        activation.env.borrow_mut().insert(local.name.clone(), val);
    }

    match eval_body(&block.lit.body, &activation, interp) {
        Ok(v) => Ok(v),
        Err(EgoSignal::NonLocalReturn(target, val)) => {
            if interp.live_activations.contains(&target) {
                Err(EgoSignal::NonLocalReturn(target, val))
            } else {
                Err(EgoSignal::Err(EgoError::new(
                    span.clone(),
                    "non-local return to a dead activation (badBlockActivation)".into(),
                )))
            }
        }
        Err(e) => Err(e),
    }
}

fn eval_body(stmts: &[Stmt], activation: &Activation, interp: &mut Interpreter) -> EvalResult {
    let mut last = interp.roots.nil_id;
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Expr(expr) => {
                last = eval_expr(expr, activation, interp)?;
            }
            StmtKind::Return(expr) => {
                let val = eval_expr(expr, activation, interp)?;
                return Err(EgoSignal::NonLocalReturn(activation.id, val));
            }
        }
    }
    Ok(last)
}

pub fn eval_program(
    program: &Program,
    self_obj: ObjectId,
    interp: &mut Interpreter,
) -> EvalResult {
    let id = interp.next_activation_id();
    interp.live_activations.insert(id);
    let activation = Activation { id, self_obj, resend_start: None, env: env_new() };
    interp.roots.activation_envs.push(activation.env.clone());
    let result = eval_body(program, &activation, interp);
    interp.roots.activation_envs.pop();
    interp.live_activations.remove(&id);

    match result {
        Ok(v) => Ok(v),
        Err(EgoSignal::NonLocalReturn(target, val)) if target == id => Ok(val),
        Err(e) => Err(e),
    }
}

pub fn eval_expr(
    expr: &Expr,
    activation: &Activation,
    interp: &mut Interpreter,
) -> EvalResult {
    match &expr.kind {
        ExprKind::Int(n) => {
            let id = alloc_with_gc(
                &mut interp.arena,
                &interp.roots,
                Object::new(ObjectKind::Integer(*n)),
            );
            interp.arena.get_mut(id).slots.push(Slot {
                name: "parent*".to_string(),
                kind: SlotKind::Parent,
                value: interp.roots.integer_proto,
            });
            Ok(id)
        }
        ExprKind::Float(f) => {
            let id = alloc_with_gc(
                &mut interp.arena,
                &interp.roots,
                Object::new(ObjectKind::Float(*f)),
            );
            interp.arena.get_mut(id).slots.push(Slot {
                name: "parent*".to_string(),
                kind: SlotKind::Parent,
                value: interp.roots.float_proto,
            });
            Ok(id)
        }
        ExprKind::Str(s) => {
            let id = alloc_with_gc(
                &mut interp.arena,
                &interp.roots,
                Object::new(ObjectKind::StringVal(s.as_str().into())),
            );
            interp.arena.get_mut(id).slots.push(Slot {
                name: "parent*".to_string(),
                kind: SlotKind::Parent,
                value: interp.roots.string_proto,
            });
            Ok(id)
        }

        ExprKind::Self_ => Ok(activation.self_obj),

        ExprKind::Assign { name, value } => {
            let val = eval_expr(value, activation, interp)?;
            activation.env.borrow_mut().insert(name.clone(), val);
            Ok(val)
        }

        ExprKind::ResendSend { target, sel, args } => {
            let roots_base = interp.roots.stack_roots.len();
            let mut arg_ids = Vec::with_capacity(args.len());
            for a in args {
                match eval_expr(a, activation, interp) {
                    Ok(id) => {
                        interp.roots.stack_roots.push(id);
                        arg_ids.push(id);
                    }
                    Err(e) => {
                        interp.roots.stack_roots.truncate(roots_base);
                        return Err(e);
                    }
                }
            }
            interp.roots.stack_roots.truncate(roots_base);
            eval_resend(target, sel, &arg_ids, activation, &expr.span, interp)
        }

        ExprKind::Ident(name) => {
            if let Some(&val) = activation.env.borrow().get(name.as_str()) {
                return Ok(val);
            }
            // Bare identifiers are implicit unary sends to self.
            // At top-level self_obj is the lobby, so lobby slots are found here too.
            eval_send(activation.self_obj, name, &[], &expr.span, interp)
        }

        ExprKind::UnarySend { recv, sel } => {
            let recv_id = eval_expr(recv, activation, interp)?;
            eval_send(recv_id, sel, &[], &expr.span, interp)
        }

        ExprKind::BinarySend { recv, sel, arg } => {
            let recv_id = eval_expr(recv, activation, interp)?;
            // Protect recv_id while evaluating the argument.
            interp.roots.stack_roots.push(recv_id);
            let arg_id = eval_expr(arg, activation, interp);
            interp.roots.stack_roots.pop();
            let arg_id = arg_id?;
            eval_send(recv_id, sel, &[arg_id], &expr.span, interp)
        }

        ExprKind::KeywordSend { recv, sel, args } => {
            let recv_id = eval_expr(recv, activation, interp)?;
            let roots_base = interp.roots.stack_roots.len();
            interp.roots.stack_roots.push(recv_id);
            let mut arg_ids = Vec::with_capacity(args.len());
            for a in args {
                let arg_id = eval_expr(a, activation, interp);
                match arg_id {
                    Ok(id) => {
                        interp.roots.stack_roots.push(id);
                        arg_ids.push(id);
                    }
                    Err(e) => {
                        interp.roots.stack_roots.truncate(roots_base);
                        return Err(e);
                    }
                }
            }
            interp.roots.stack_roots.truncate(roots_base);
            eval_send(recv_id, sel, &arg_ids, &expr.span, interp)
        }

        ExprKind::Cascade { recv, msgs } => {
            let recv_id = eval_expr(recv, activation, interp)?;
            let mut last = interp.roots.nil_id;
            for msg in msgs {
                last = match msg {
                    CascadeMsg::Unary { sel, span } => {
                        eval_send(recv_id, sel, &[], span, interp)?
                    }
                    CascadeMsg::Binary { sel, arg, span } => {
                        interp.roots.stack_roots.push(recv_id);
                        let arg_id = eval_expr(arg, activation, interp);
                        interp.roots.stack_roots.pop();
                        eval_send(recv_id, sel, &[arg_id?], span, interp)?
                    }
                    CascadeMsg::Keyword { sel, args, span } => {
                        let roots_base = interp.roots.stack_roots.len();
                        interp.roots.stack_roots.push(recv_id);
                        let mut arg_ids = Vec::with_capacity(args.len());
                        let mut err = None;
                        for a in args {
                            match eval_expr(a, activation, interp) {
                                Ok(id) => {
                                    interp.roots.stack_roots.push(id);
                                    arg_ids.push(id);
                                }
                                Err(e) => { err = Some(e); break; }
                            }
                        }
                        interp.roots.stack_roots.truncate(roots_base);
                        if let Some(e) = err { return Err(e); }
                        eval_send(recv_id, sel, &arg_ids, span, interp)?
                    }
                };
            }
            Ok(last)
        }

        ExprKind::Block(lit) => {
            let block_data = BlockData {
                lit: lit.clone(),
                home_id: activation.id,
                captured_self: activation.self_obj,
                captured_resend: activation.resend_start,
                captures: activation.env.clone(),
            };
            let id = alloc_with_gc(
                &mut interp.arena,
                &interp.roots,
                Object::new(ObjectKind::Block(Box::new(block_data))),
            );
            interp.arena.get_mut(id).slots.push(Slot {
                name: "parent*".to_string(),
                kind: SlotKind::Parent,
                value: interp.roots.block_proto,
            });
            Ok(id)
        }

        ExprKind::Object(obj) => eval_object_lit(obj, activation, interp),
    }
}

fn eval_object_lit(obj: &ObjectLit, activation: &Activation, interp: &mut Interpreter) -> EvalResult {
    let new_id = alloc_with_gc(&mut interp.arena, &interp.roots, Object::new(ObjectKind::Plain));
    let roots_base = interp.roots.stack_roots.len();
    interp.roots.stack_roots.push(new_id);
    let result = eval_object_slots(obj, new_id, activation, interp);
    interp.roots.stack_roots.truncate(roots_base);
    result
}

fn eval_object_slots(
    obj: &ObjectLit,
    new_id: ObjectId,
    activation: &Activation,
    interp: &mut Interpreter,
) -> EvalResult {
    for decl in &obj.slots {
        match &decl.kind {
            SlotDeclKind::Data { name, value } => {
                let val = eval_expr(value, activation, interp)?;
                interp.arena.get_mut(new_id).slots.push(Slot {
                    name: name.clone(),
                    kind: SlotKind::Data,
                    value: val,
                });
            }
            SlotDeclKind::Var { name, value } => {
                let val = eval_expr(value, activation, interp)?;
                interp.arena.get_mut(new_id).slots.push(Slot {
                    name: name.clone(),
                    kind: SlotKind::Var,
                    value: val,
                });
                // `val` is now reachable through `new_id`'s slots, so this
                // alloc (which may trigger GC) is safe.
                let setter_id = alloc_with_gc(
                    &mut interp.arena,
                    &interp.roots,
                    Object::new(ObjectKind::VarSetter(name.clone())),
                );
                interp.arena.get_mut(new_id).slots.push(Slot {
                    name: format!("{name}:"),
                    kind: SlotKind::Method,
                    value: setter_id,
                });
            }
            SlotDeclKind::Parent { name, value } => {
                let val = eval_expr(value, activation, interp)?;
                interp.arena.get_mut(new_id).slots.push(Slot {
                    name: name.clone(),
                    kind: SlotKind::Parent,
                    value: val,
                });
            }
            SlotDeclKind::Arg { name } => {
                interp.arena.get_mut(new_id).slots.push(Slot {
                    name: name.clone(),
                    kind: SlotKind::Arg,
                    value: interp.roots.nil_id,
                });
            }
            SlotDeclKind::Method { sel, body } => {
                let method_def = Rc::new(MethodDef {
                    params: sel.params(),
                    body: body.clone(),
                    source: decl.span.clone(),
                });
                let method_obj = alloc_with_gc(
                    &mut interp.arena,
                    &interp.roots,
                    Object::new(ObjectKind::Method(method_def)),
                );
                interp.arena.get_mut(new_id).slots.push(Slot {
                    name: sel.selector(),
                    kind: SlotKind::Method,
                    value: method_obj,
                });
            }
        }
    }
    Ok(new_id)
}

/// Lex, parse, eval source in auto-print mode: returns the `printString` of the
/// last expression, or `None` if the source is empty.
pub fn eval_source_print(
    source: &str,
    filename: &str,
    interp: &mut Interpreter,
) -> Result<Option<String>, EgoSignal> {
    let file = Rc::new(filename.to_string());
    let tokens = lex(source, file.clone()).map_err(EgoSignal::Err)?;
    let program = parse(&tokens, file.clone()).map_err(EgoSignal::Err)?;
    if program.is_empty() {
        return Ok(None);
    }
    let lobby = interp.roots.lobby;
    let result = eval_program(&program, lobby, interp)?;
    let span = SourceSpan::new(file, 0, 0);
    let str_id = eval_send(result, "printString", &[], &span, interp)?;
    match &interp.arena.get(str_id).kind {
        ObjectKind::StringVal(s) => Ok(Some(s.to_string())),
        _ => Err(EgoSignal::Err(EgoError::new(span, "printString returned non-string".into()))),
    }
}

/// Lex, parse, and eval source in script mode (no auto-print).
pub fn eval_source_run(
    source: &str,
    filename: &str,
    interp: &mut Interpreter,
) -> Result<(), EgoSignal> {
    let file = Rc::new(filename.to_string());
    let tokens = lex(source, file.clone()).map_err(EgoSignal::Err)?;
    let program = parse(&tokens, file).map_err(EgoSignal::Err)?;
    if !program.is_empty() {
        let lobby = interp.roots.lobby;
        eval_program(&program, lobby, interp)?;
    }
    Ok(())
}
