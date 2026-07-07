use std::rc::Rc;

use crate::ast::*;
use crate::error::{EgoError, SourceSpan};
use crate::lexer::{Token, TokenWithSpan};

pub fn parse(tokens: &[TokenWithSpan], file: Rc<String>) -> Result<Program, EgoError> {
    Parser::new(tokens, file).parse_program()
}

// ── Parser ─────────────────────────────────────────────────────────────────

struct Parser<'t> {
    tokens:      &'t [TokenWithSpan],
    pos:         usize,
    file:        Rc<String>,
    stop_at_bar: bool,
}

impl<'t> Parser<'t> {
    fn new(tokens: &'t [TokenWithSpan], file: Rc<String>) -> Self {
        Self { tokens, pos: 0, file, stop_at_bar: false }
    }

    // ── Primitives ─────────────────────────────────────────────────────────

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos).map(|t| &t.token)
    }

    fn peek_at(&self, offset: usize) -> Option<&Token> {
        self.tokens.get(self.pos + offset).map(|t| &t.token)
    }

    fn span(&self) -> SourceSpan {
        self.tokens
            .get(self.pos)
            .map(|t| t.span.clone())
            .unwrap_or_else(|| SourceSpan::new(self.file.clone(), 0, 0))
    }

    fn advance(&mut self) -> &'t TokenWithSpan {
        let t = &self.tokens[self.pos];
        self.pos += 1;
        t
    }

    fn at_end(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    fn err(&self, span: SourceSpan, msg: impl Into<String>) -> EgoError {
        EgoError::new(span, msg.into())
    }

    fn expect_ident(&mut self) -> Result<(String, SourceSpan), EgoError> {
        let span = self.span();
        match self.peek().cloned() {
            Some(Token::Ident(s)) => { self.advance(); Ok((s, span)) }
            _ => Err(self.err(span, "expected identifier")),
        }
    }

    fn expect_str_lit(&mut self) -> Result<(String, SourceSpan), EgoError> {
        let span = self.span();
        match self.peek().cloned() {
            Some(Token::Str(s)) => { self.advance(); Ok((s, span)) }
            _ => Err(self.err(span, "expected string literal")),
        }
    }

    fn expect_eq(&mut self) -> Result<SourceSpan, EgoError> {
        let span = self.span();
        if is_eq(self.peek()) { self.advance(); Ok(span) }
        else { Err(self.err(span, "expected '='")) }
    }

    fn expect_lparen(&mut self) -> Result<SourceSpan, EgoError> {
        let span = self.span();
        if self.peek() == Some(&Token::LParen) { self.advance(); Ok(span) }
        else { Err(self.err(span, "expected '('")) }
    }

    fn expect_rparen(&mut self) -> Result<(), EgoError> {
        if self.peek() == Some(&Token::RParen) { self.advance(); Ok(()) }
        else { Err(self.err(self.span(), "expected ')'")) }
    }

    fn expect_rbracket(&mut self) -> Result<(), EgoError> {
        if self.peek() == Some(&Token::RBrack) { self.advance(); Ok(()) }
        else { Err(self.err(self.span(), "expected ']'")) }
    }

    fn expect_bar(&mut self) -> Result<(), EgoError> {
        if is_bar(self.peek()) { self.advance(); Ok(()) }
        else { Err(self.err(self.span(), "expected '|'")) }
    }

    fn expect_dot(&mut self) -> Result<(), EgoError> {
        if self.peek() == Some(&Token::Dot) { self.advance(); Ok(()) }
        else { Err(self.err(self.span(), "expected '.'")) }
    }

    // Parse a dot-separated statement list. Stops (without consuming) when
    // `stop` returns true for the current token, or at EOF.
    fn parse_stmts<F>(&mut self, stop: F) -> Result<Vec<Stmt>, EgoError>
    where F: Fn(Option<&Token>) -> bool {
        let mut stmts = Vec::new();
        loop {
            if stop(self.peek()) || self.at_end() { break; }
            stmts.push(self.parse_stmt()?);
            if self.peek() == Some(&Token::Dot) {
                self.advance();
            } else if !stop(self.peek()) && !self.at_end() {
                return Err(self.err(self.span(), "expected '.'"));
            }
        }
        Ok(stmts)
    }

    // ── Program ────────────────────────────────────────────────────────────

    fn parse_program(&mut self) -> Result<Program, EgoError> {
        self.parse_stmts(|t| t.is_none())
    }

    // ── Statements ─────────────────────────────────────────────────────────

    fn parse_stmt(&mut self) -> Result<Stmt, EgoError> {
        let span = self.span();
        if self.peek() == Some(&Token::Caret) {
            self.advance();
            let expr = self.parse_cascade()?;
            Ok(Stmt { kind: StmtKind::Return(Box::new(expr)), span })
        } else {
            let expr = self.parse_cascade()?;
            let span = expr.span.clone();
            Ok(Stmt { kind: StmtKind::Expr(Box::new(expr)), span })
        }
    }

    // ── Cascade ────────────────────────────────────────────────────────────

    fn parse_cascade(&mut self) -> Result<Expr, EgoError> {
        let recv = self.parse_keyword()?;
        if self.peek() != Some(&Token::Semi) {
            return Ok(recv);
        }
        let span = recv.span.clone();
        // `recv` is the full first send (e.g. `a foo`); peel its outermost
        // message off so every cascade message — including this one — is
        // sent to the same shared receiver (`a`), not to the first send's
        // result.
        let (base_recv, mut msgs) = split_cascade_head(recv);
        while self.peek() == Some(&Token::Semi) {
            self.advance();
            msgs.push(self.parse_cascade_msg()?);
        }
        Ok(Expr { kind: ExprKind::Cascade { recv: Box::new(base_recv), msgs }, span })
    }

    fn parse_cascade_msg(&mut self) -> Result<CascadeMsg, EgoError> {
        let span = self.span();
        match self.peek().cloned() {
            Some(Token::Ident(sel)) => {
                self.advance();
                Ok(CascadeMsg::Unary { sel, span })
            }
            Some(Token::Keyword(_)) => {
                let (sel, args) = self.parse_keyword_chain()?;
                Ok(CascadeMsg::Keyword { sel, args, span })
            }
            Some(Token::Binary(sel)) => {
                self.advance();
                let arg = self.parse_unary()?;
                Ok(CascadeMsg::Binary { sel, arg, span })
            }
            _ => Err(self.err(span, "expected cascade message (unary, binary, or keyword)")),
        }
    }

    // ── Keyword / Binary / Unary ───────────────────────────────────────────

    fn parse_keyword(&mut self) -> Result<Expr, EgoError> {
        let recv = self.parse_binary()?;
        // A keyword message may only *start* with a lowercase-initial part;
        // a bare `CapKeyword` here belongs to something else entirely (or is
        // an error the caller will report), not the start of a new message.
        if !matches!(self.peek(), Some(Token::Keyword(_))) {
            return Ok(recv);
        }
        self.parse_keyword_tail(recv)
    }

    /// Parses one or more `(keyword_part | cap_keyword_part) BinaryExpr` pairs
    /// onto `recv`, given that the next token is already known to be a
    /// lowercase-initial `keyword_part`.
    ///
    /// A `cap_keyword_part` continues the message in progress. A
    /// lowercase-initial `keyword_part` instead closes it: the value just
    /// parsed as its argument is reinterpreted as the *receiver* of a new,
    /// nested keyword message, built by recursing into this same function.
    /// This is what lets keyword sends chain right-to-left with no
    /// parentheses — see lang-spec.md §2 for the worked example.
    fn parse_keyword_tail(&mut self, recv: Expr) -> Result<Expr, EgoError> {
        let span = recv.span.clone();
        let (sel, args) = self.parse_keyword_chain()?;
        Ok(Expr { kind: ExprKind::KeywordSend { recv: Box::new(recv), sel, args }, span })
    }

    /// Core of `parse_keyword_tail`, factored out so cascades (which need the
    /// flat `(sel, args)` pair rather than a `KeywordSend` wrapping a real
    /// receiver) can share the same grouping logic.
    fn parse_keyword_chain(&mut self) -> Result<(String, Vec<Expr>), EgoError> {
        let mut sel = String::new();
        let mut args = Vec::new();
        loop {
            let kw = match self.advance().token.clone() {
                Token::Keyword(s) | Token::CapKeyword(s) => s,
                _ => unreachable!("caller/loop guarantees a keyword part here"),
            };
            sel.push_str(&kw);
            let arg = self.parse_binary()?;
            if matches!(self.peek(), Some(Token::Keyword(_))) {
                args.push(self.parse_keyword_tail(arg)?);
                break;
            }
            args.push(arg);
            if !matches!(self.peek(), Some(Token::CapKeyword(_))) {
                break;
            }
        }
        Ok((sel, args))
    }

    fn parse_binary(&mut self) -> Result<Expr, EgoError> {
        let mut recv = self.parse_unary()?;
        let mut chain_sel: Option<String> = None;
        while let Some(Token::Binary(sel)) = self.peek().cloned() {
            if self.stop_at_bar && sel == "|" { break; }
            let span = self.span();
            if let Some(first) = &chain_sel {
                if *first != sel {
                    return Err(self.err(
                        span,
                        format!(
                            "cannot mix binary operators '{first}' and '{sel}' in one \
                             expression without parentheses"
                        ),
                    ));
                }
            } else {
                chain_sel = Some(sel.clone());
            }
            self.advance();
            let arg = self.parse_unary()?;
            recv = Expr {
                kind: ExprKind::BinarySend { recv: Box::new(recv), sel, arg: Box::new(arg) },
                span,
            };
        }
        Ok(recv)
    }

    fn parse_unary(&mut self) -> Result<Expr, EgoError> {
        let mut recv = self.parse_primary()?;
        while let Some(Token::Ident(sel)) = self.peek().cloned() {
            let span = self.span();
            self.advance();
            recv = Expr { kind: ExprKind::UnarySend { recv: Box::new(recv), sel }, span };
        }
        Ok(recv)
    }

    // ── Primary ────────────────────────────────────────────────────────────

    fn parse_primary(&mut self) -> Result<Expr, EgoError> {
        // Entering any primary suspends the slot-value | terminator: inside
        // grouped contexts (parens, blocks, object literals) the | is not a
        // slot-list terminator and may be a binary operator.
        let was_stop = self.stop_at_bar;
        self.stop_at_bar = false;
        let result = self.parse_primary_inner();
        self.stop_at_bar = was_stop;
        result
    }

    fn parse_primary_inner(&mut self) -> Result<Expr, EgoError> {
        let span = self.span();
        match self.peek().cloned() {
            Some(Token::Integer(n)) => { self.advance(); Ok(Expr { kind: ExprKind::Int(n),   span }) }
            Some(Token::Float(f))   => { self.advance(); Ok(Expr { kind: ExprKind::Float(f), span }) }
            Some(Token::Str(s))     => { self.advance(); Ok(Expr { kind: ExprKind::Str(s),   span }) }
            Some(Token::Ident(s))   => { self.advance(); Ok(Expr { kind: ExprKind::Ident(s), span }) }
            Some(Token::Self_)      => { self.advance(); Ok(Expr { kind: ExprKind::Self_,     span }) }
            Some(Token::Resend)     => { self.advance(); Ok(Expr { kind: ExprKind::Resend,    span }) }
            Some(Token::LBrack)     => self.parse_block_lit(),
            Some(Token::LParen) => {
                // ( | ... ) → object literal; ( expr ) → parenthesised expression
                if is_bar(self.peek_at(1)) {
                    self.parse_object_lit()
                } else {
                    self.advance(); // consume '('
                    let expr = self.parse_keyword()?;
                    self.expect_rparen()?;
                    Ok(expr)
                }
            }
            _ => Err(self.err(span, "expected expression")),
        }
    }

    // ── Object literal ─────────────────────────────────────────────────────

    fn parse_object_lit(&mut self) -> Result<Expr, EgoError> {
        let span = self.span();
        self.advance(); // '('
        self.advance(); // opening '|'

        // Optional annotation: {} = 'string' .
        let annotation = if self.peek() == Some(&Token::LBrace) {
            self.advance(); // '{'
            if self.peek() != Some(&Token::RBrace) {
                return Err(self.err(self.span(), "expected '}'"));
            }
            self.advance(); // '}'
            self.expect_eq()?;
            let (s, _) = self.expect_str_lit()?;
            self.expect_dot()?;
            Some(s)
        } else {
            None
        };

        let slots = self.parse_slot_list()?;
        self.expect_bar()?; // closing '|'

        let body = self.parse_stmts(|t| matches!(t, Some(Token::RParen) | None))?;
        self.expect_rparen()?;

        Ok(Expr {
            kind: ExprKind::Object(Box::new(ObjectLit { annotation, slots, body, span: span.clone() })),
            span,
        })
    }

    fn parse_slot_list(&mut self) -> Result<Vec<SlotDecl>, EgoError> {
        let mut slots = Vec::new();
        loop {
            if is_bar(self.peek()) || self.at_end() { break; }
            slots.push(self.parse_slot_decl()?);
            if self.peek() == Some(&Token::Dot) {
                self.advance();
            } else if !is_bar(self.peek()) {
                return Err(self.err(self.span(), "expected '.' or '|' after slot declaration"));
            }
        }
        Ok(slots)
    }

    fn parse_slot_decl(&mut self) -> Result<SlotDecl, EgoError> {
        let span = self.span();

        match self.peek().cloned() {
            // ArgSlotDecl: : identifier
            Some(Token::Colon) => {
                self.advance();
                let (name, _) = self.expect_ident()?;
                Ok(SlotDecl { kind: SlotDeclKind::Arg { name }, span })
            }

            // Starts with identifier: Data | Var | Parent | unary Method
            Some(Token::Ident(name)) => {
                match self.peek_at(1) {
                    // VarSlotDecl: ident <- expr
                    Some(Token::Binary(s)) if s == "<-" => {
                        self.advance(); self.advance(); // ident "<-"
                        self.stop_at_bar = true;
                        let value = self.parse_keyword();
                        self.stop_at_bar = false;
                        Ok(SlotDecl { kind: SlotDeclKind::Var { name, value: value? }, span })
                    }
                    // ParentSlotDecl: ident * = expr
                    Some(Token::Binary(s)) if s == "*" => {
                        if !is_eq(self.peek_at(2)) {
                            return Err(self.err(span, "expected '=' after '*' in parent slot"));
                        }
                        self.advance(); self.advance(); self.advance(); // ident "*" "="
                        self.stop_at_bar = true;
                        let value = self.parse_keyword();
                        self.stop_at_bar = false;
                        Ok(SlotDecl { kind: SlotDeclKind::Parent { name, value: value? }, span })
                    }
                    // DataSlotDecl or unary MethodSlotDecl: ident = ...
                    Some(Token::Binary(s)) if s == "=" => {
                        // Unary method when: ident "=" "(" and "(" is NOT followed by "|"
                        let is_method = matches!(self.peek_at(2), Some(Token::LParen))
                            && !is_bar(self.peek_at(3));
                        self.advance(); self.advance(); // ident "="
                        if is_method {
                            self.advance(); // "("
                            let body = self.parse_stmts(|t| matches!(t, Some(Token::RParen) | None))?;
                            self.expect_rparen()?;
                            Ok(SlotDecl { kind: SlotDeclKind::Method { sel: MethodSel::Unary(name), body }, span })
                        } else {
                            self.stop_at_bar = true;
                            let value = self.parse_keyword();
                            self.stop_at_bar = false;
                            Ok(SlotDecl { kind: SlotDeclKind::Data { name, value: value? }, span })
                        }
                    }
                    _ => Err(self.err(span, format!("expected '=', '<-', or '*' after slot name '{name}'"))),
                }
            }

            // Binary MethodSlotDecl: sel param = ( body )
            Some(Token::Binary(sel)) => {
                self.advance(); // sel
                let (param, _) = self.expect_ident()?;
                self.expect_eq()?;
                self.expect_lparen()?;
                let body = self.parse_stmts(|t| matches!(t, Some(Token::RParen) | None))?;
                self.expect_rparen()?;
                Ok(SlotDecl { kind: SlotDeclKind::Method { sel: MethodSel::Binary(sel, param), body }, span })
            }

            // Keyword MethodSlotDecl: kw1 p1 kw2 p2 ... = ( body )
            Some(Token::Keyword(_)) | Some(Token::CapKeyword(_)) => {
                let mut parts: Vec<(String, String)> = Vec::new();
                while matches!(self.peek(), Some(Token::Keyword(_)) | Some(Token::CapKeyword(_))) {
                    let kw = match self.advance().token.clone() {
                        Token::Keyword(s) | Token::CapKeyword(s) => s,
                        _ => unreachable!(),
                    };
                    let (param, _) = self.expect_ident()?;
                    parts.push((kw, param));
                }
                self.expect_eq()?;
                self.expect_lparen()?;
                let body = self.parse_stmts(|t| matches!(t, Some(Token::RParen) | None))?;
                self.expect_rparen()?;
                Ok(SlotDecl { kind: SlotDeclKind::Method { sel: MethodSel::Keyword(parts), body }, span })
            }

            _ => Err(self.err(span, "expected slot declaration")),
        }
    }

    // ── Block literal ──────────────────────────────────────────────────────

    fn parse_block_lit(&mut self) -> Result<Expr, EgoError> {
        let span = self.span();
        self.advance(); // '['

        let (params, locals) = if is_bar(self.peek()) {
            self.advance(); // opening '|'
            let r = self.parse_block_slots()?;
            self.expect_bar()?; // closing '|'
            r
        } else {
            (Vec::new(), Vec::new())
        };

        let body = self.parse_stmts(|t| matches!(t, Some(Token::RBrack) | None))?;
        self.expect_rbracket()?;

        Ok(Expr {
            kind: ExprKind::Block(Box::new(BlockLit { params, locals, body, span: span.clone() })),
            span,
        })
    }

    fn parse_block_slots(&mut self) -> Result<(Vec<String>, Vec<BlockLocal>), EgoError> {
        let mut params = Vec::new();
        let mut locals = Vec::new();
        loop {
            if is_bar(self.peek()) || self.at_end() { break; }
            let span = self.span();
            match self.peek().cloned() {
                // ArgSlotDecl: :ident
                Some(Token::Colon) => {
                    self.advance();
                    let (name, _) = self.expect_ident()?;
                    params.push(name);
                }
                // DataSlotDecl: ident = expr
                Some(Token::Ident(name)) if matches!(self.peek_at(1), Some(Token::Binary(s)) if s == "=") => {
                    self.advance(); self.advance(); // ident "="
                    self.stop_at_bar = true;
                    let init = self.parse_keyword();
                    self.stop_at_bar = false;
                    locals.push(BlockLocal { name, kind: LocalKind::Data, init: init? });
                }
                // VarSlotDecl: ident <- expr
                Some(Token::Ident(name)) if matches!(self.peek_at(1), Some(Token::Binary(s)) if s == "<-") => {
                    self.advance(); self.advance(); // ident "<-"
                    self.stop_at_bar = true;
                    let init = self.parse_keyword();
                    self.stop_at_bar = false;
                    locals.push(BlockLocal { name, kind: LocalKind::Var, init: init? });
                }
                _ => return Err(self.err(span, "expected block slot declaration (':name', 'name = expr', or 'name <- expr')")),
            }
            if self.peek() == Some(&Token::Dot) {
                self.advance();
            } else if !is_bar(self.peek()) {
                return Err(self.err(self.span(), "expected '.' or '|'"));
            }
        }
        Ok((params, locals))
    }
}

// ── Cascade helpers ─────────────────────────────────────────────────────────

/// If `expr` is itself a message send, peel off its outermost send so the
/// send's receiver becomes the shared cascade receiver and the send itself
/// becomes the first cascade message. Otherwise `expr` has no leading send
/// to peel (e.g. a bare identifier before `;`), so it is the receiver as-is.
fn split_cascade_head(expr: Expr) -> (Expr, Vec<CascadeMsg>) {
    let span = expr.span.clone();
    match expr.kind {
        ExprKind::UnarySend { recv, sel } => (*recv, vec![CascadeMsg::Unary { sel, span }]),
        ExprKind::BinarySend { recv, sel, arg } => {
            (*recv, vec![CascadeMsg::Binary { sel, arg: *arg, span }])
        }
        ExprKind::KeywordSend { recv, sel, args } => {
            (*recv, vec![CascadeMsg::Keyword { sel, args, span }])
        }
        kind => (Expr { kind, span }, Vec::new()),
    }
}

// ── Token predicate helpers ────────────────────────────────────────────────

fn is_bar(t: Option<&Token>) -> bool {
    matches!(t, Some(Token::Binary(s)) if s == "|")
}

fn is_eq(t: Option<&Token>) -> bool {
    matches!(t, Some(Token::Binary(s)) if s == "=")
}
