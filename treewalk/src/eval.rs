use std::rc::Rc;

use crate::arena::ObjectId;
use crate::ast::{CascadeMsg, Expr, ExprKind, Program, Stmt, StmtKind};
use crate::bootstrap::Interpreter;
use crate::env::{ActivationId, Env, env_new};
use crate::error::{EgoError, SourceSpan};
use crate::gc::alloc_with_gc;
use crate::lexer::lex;
use crate::object::{MethodDef, Object, ObjectKind, Slot, SlotKind};
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
    Method(Rc<MethodDef>),
    Value(ObjectId),
}

fn lookup_slot(recv: ObjectId, sel: &str, interp: &Interpreter) -> Option<SlotLookup> {
    lookup_in(recv, sel, interp, &mut Vec::new())
}

fn lookup_in(
    id: ObjectId,
    sel: &str,
    interp: &Interpreter,
    visited: &mut Vec<ObjectId>,
) -> Option<SlotLookup> {
    if visited.contains(&id) {
        return None;
    }
    visited.push(id);

    let obj = interp.arena.get(id);
    for slot in &obj.slots {
        if slot.name == sel {
            return match &slot.kind {
                SlotKind::Method => {
                    if let ObjectKind::Method(m) = &interp.arena.get(slot.value).kind {
                        Some(SlotLookup::Method(m.clone()))
                    } else {
                        None
                    }
                }
                SlotKind::Data | SlotKind::Var => Some(SlotLookup::Value(slot.value)),
                _ => None,
            };
        }
    }

    let parents: Vec<ObjectId> = obj
        .slots
        .iter()
        .filter(|s| s.kind == SlotKind::Parent)
        .map(|s| s.value)
        .collect();

    for parent_id in parents {
        if let Some(result) = lookup_in(parent_id, sel, interp, visited) {
            return Some(result);
        }
    }
    None
}

pub fn eval_send(
    recv: ObjectId,
    sel: &str,
    args: &[ObjectId],
    span: &SourceSpan,
    interp: &mut Interpreter,
) -> EvalResult {
    if sel.starts_with('_') {
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
        let result = prim_fn(recv, args, &mut interp.arena, &mut interp.roots)
            .map_err(EgoSignal::Err);
        interp.roots.stack_roots.truncate(roots_base);
        return result;
    }

    match lookup_slot(recv, sel, interp) {
        Some(SlotLookup::Method(method_def)) => {
            eval_method(recv, None, method_def, args, span, interp)
        }
        Some(SlotLookup::Value(val)) => Ok(val),
        None => Err(EgoSignal::Err(EgoError::new(
            span.clone(),
            format!("message not understood: {sel}"),
        ))),
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

    // Protect self_obj and args across the method body, which may alloc.
    let roots_base = interp.roots.stack_roots.len();
    interp.roots.stack_roots.push(self_obj);
    for &arg in args {
        interp.roots.stack_roots.push(arg);
    }
    let result = eval_body(&method_def.body, &activation, interp);
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
    let result = eval_body(program, &activation, interp);
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
                value: interp.integer_proto,
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
                value: interp.float_proto,
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
                value: interp.string_proto,
            });
            Ok(id)
        }

        ExprKind::Self_ => Ok(activation.self_obj),

        ExprKind::Resend => match activation.resend_start {
            Some(obj) => Ok(obj),
            None => Err(EgoSignal::Err(EgoError::new(
                expr.span.clone(),
                "resend used outside a method".into(),
            ))),
        },

        ExprKind::Ident(name) => {
            if let Some(&val) = activation.env.borrow().get(name.as_str()) {
                return Ok(val);
            }
            let lobby = interp.roots.lobby;
            let found = interp
                .arena
                .get(lobby)
                .slots
                .iter()
                .find(|s| s.name == *name && matches!(s.kind, SlotKind::Data | SlotKind::Var))
                .map(|s| s.value);
            match found {
                Some(val) => Ok(val),
                None => Err(EgoSignal::Err(EgoError::new(
                    expr.span.clone(),
                    format!("undefined variable: {name}"),
                ))),
            }
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

        ExprKind::Block(_) | ExprKind::Object(_) => Err(EgoSignal::Err(EgoError::new(
            expr.span.clone(),
            "blocks and object literals not yet implemented".into(),
        ))),
    }
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
