# Stage 1: Rust Tree-Walking Interpreter

Design document for the Rust tree-walking interpreter. Covers crate layout,
object model, GC, primitive dispatch, bootstrap, evaluator, REPL, diagnostics,
and testing. Corresponds to ROADMAP substages 1.1–1.18.

**Scope:** No image. Every run parses source, loads `boot.ego`, evaluates, and
discards the object graph on exit. The object graph lives in a managed arena; a
mark-and-sweep GC handles the cyclic parent chains inherent in a prototype-based
system.

---

## Crate layout

Single crate. Modules are one file each; add submodules only if a file exceeds
roughly 600 lines.

```
ego/
  Cargo.toml
  boot/
    boot.ego            ← standard library and traits, in ego itself
  src/
    main.rs             ← CLI arg parsing; REPL loop; script/eval dispatch
    error.rs            ← EgoError, source location, diagnostics
    lexer.rs            ← tokeniser
    ast.rs              ← AST node types
    parser.rs           ← recursive-descent parser → AST
    arena.rs            ← Arena, ObjectId
    object.rs           ← Object, ObjectKind, Slot, SlotKind
    gc.rs               ← mark-and-sweep collector
    env.rs              ← Env (lexical scope frame), ActivationId
    eval.rs             ← tree-walking evaluator
    primitives.rs       ← primitive table; _-selector dispatch
    bootstrap.rs        ← startup sequence; boot.ego loading
  tests/
    lexer_tests.rs
    parser_tests.rs
    eval_golden.rs      ← golden test harness
    eval_golden/        ← .ego input files and .expected output files
```

`boot.ego` is embedded into the binary at compile time via
`include_str!("../boot/boot.ego")`. This avoids install-path issues; the
interpreter is a single self-contained binary.

Crate uses Rust edition 2024. Since the binary name (`ego`) differs from the
package name (`treewalk`), `[[bin]]` must set `path = "src/main.rs"`
explicitly in `Cargo.toml` — edition 2024 no longer infers `src/main.rs` in
that case.

---

## Object model

### Arena and ObjectId

All ego objects live in a single flat `Vec`. References between objects are
expressed as integer indices — not Rust references — so the borrow checker has
nothing to say about the cyclic parent chains that a prototype-based object
model requires.

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ObjectId(u32);

pub struct Arena {
    objects: Vec<Object>,
    free:    Vec<u32>,   // recycled indices, maintained as a stack
}
```

`ObjectId(u32::MAX)` is reserved as a sentinel (`NULL_ID`) used only in arena
internals. It is never a valid ego value; ego's `nil` is a normal arena object
registered during bootstrap.

### Object

```rust
pub struct Object {
    pub mark:  bool,
    pub kind:  ObjectKind,
    pub slots: Vec<Slot>,
}

pub enum ObjectKind {
    Plain,
    Integer(i64),
    Float(f64),
    BigInt(Box<BigInt>),        // num-bigint crate; substage 1.18. Parent is
                                 // integer_proto, same as Integer — arithmetic
                                 // primitives normalize every result back to
                                 // Integer when it fits in i64, so a value has
                                 // exactly one canonical representation.
    StringVal(Box<str>),
    Method(Rc<MethodDef>),
    Block(Box<BlockData>),
    Array(Vec<ObjectId>),       // fixed-size, added at substage 1.16
}
```

No tagged-pointer optimisation for integers in stage 1. Every object, including
small integers, is an arena entry. This simplifies every arena operation at the
cost of allocation pressure — acceptable for a reference interpreter.

### Slots

```rust
pub struct Slot {
    pub name:  String,
    pub kind:  SlotKind,
    pub value: ObjectId,
}

pub enum SlotKind {
    Data,
    Var,     // structurally identical to Data; a setter is generated at eval time
    Arg,     // parameter slot in a standalone method object
    Method,  // value's kind is ObjectKind::Method
    Parent,  // value is searched when a message is not found locally
}
```

Slot ordering within an object is declaration order. Lookup scans linearly;
objects are small enough that this is fine for stage 1.

### Methods

```rust
pub struct MethodDef {
    pub params: Vec<String>,   // in declaration order; empty for unary
    pub body:   Vec<AstNode>,  // statement list
    pub source: SourceSpan,    // for error reporting
}
```

`ObjectKind::Method(Rc<MethodDef>)` — `Rc` is used only to allow cloning a
slot value without copying the AST. The arena owns the `Object`; the `Rc` just
shares the AST body. `MethodDef` contains no `ObjectId`s and causes no GC
complications.

### Blocks

```rust
pub struct BlockData {
    pub lit:             Rc<BlockLit>,   // params, locals, body, span — shared, not re-cloned per activation
    pub home_id:         ActivationId,   // for non-local ^ return
    pub captured_self:   ObjectId,
    pub captured_resend: Option<ObjectId>,
    pub captures:        Env,            // shared lexical frame (see §Env)
}
```

Blocks close over `self`, `resend` (if inside a method), and all bindings
visible in the enclosing scope at the point the block literal is evaluated.
`home_id` is the `ActivationId` of the enclosing method; `^` inside the block
targets this activation. `lit` holds the parsed `BlockLit` (params, locals,
body) by `Rc`, so creating a `BlockData` each time the literal expression is
evaluated (e.g. inside a loop) only bumps a refcount, not a deep AST clone.

Block *activation* (sending `value`/`value:`/`value:With:`) binds params and
evaluates locals' initializers directly into `captures` — the same shared
`Env`, not a fresh child frame — then runs the body with a temporary
`Activation` whose `id` is set to `home_id` (so a `^` inside raises
`NonLocalReturn(home_id, _)`) and whose `self_obj`/`resend_start` are
restored from `captured_self`/`captured_resend`. Unlike `eval_method`, this
call never catches a `NonLocalReturn` whose target equals its own id — that
conversion-to-`Ok` belongs solely to the real `eval_method` frame further up
the Rust call stack. It only guards the dead-block case: if the `^` target is
no longer in the live-activation set, it converts to a fatal error
(`badBlockActivation`, lang-spec.md's error table) right at the block-call
site, rather than letting a raw `NonLocalReturn` escape frames that don't
know how to handle it.

This check is intentionally *lazy* — performed only when a `^` actually
fires and turns out to target a dead activation — not eager on every `value`
send. Self-notes.md §5 states the Smalltalk-80 rule more broadly ("sending
`value` to a dead block signals an error"), but that rule exists because
classic Smalltalk-80 stack-allocates method contexts, which become unsafe to
reference once popped. Ego's `Env` is always heap-allocated (`Rc<RefCell<...>>`)
and GC-tracked via `RootSet::activation_envs`/the `Block` object's own
`captures` field, so a block that never executes `^` remains perfectly safe
to invoke long after its creating method returned — the standard "closure
factory" pattern (a method returning a block that mutates a var slot it
closed over) depends on this. lang-spec.md's own error table agrees, scoping
`badBlockActivation` specifically to "a non-local `^` return targets an
already-returned method activation," not to `value` in general.

Because `_BlockValue`/`_BlockValue:`/`_BlockValue:Value:` must recursively
invoke the evaluator (`eval_body`) rather than perform a self-contained
computation, they cannot be ordinary `PrimFn`s — that type only threads
`Arena`/`RootSet`, not the full `Interpreter`. `eval_send` intercepts these
three selectors directly (checking the receiver is `ObjectKind::Block`)
before falling through to the primitive table, rooting `recv`/`args` the
same way the primitive-dispatch path already does. `boot.ego`'s `value`/
`value:`/`value:With:` methods on `blockProto`'s trait forward to them
exactly like `+` forwards to `_IntAdd:` — the interception is invisible to
ego-level code.

---

## Parser

Recursive-descent, hand-written. Public entry point:

```rust
pub fn parse(tokens: &[TokenWithSpan], file: Rc<String>) -> Result<Program, EgoError>
```

`Program` is `Vec<Stmt>`. A `Parser` struct holds a slice reference to the
token stream and a position index; it never backtracks.

### AST types (`ast.rs`)

| Type | Description |
|---|---|
| `Stmt` / `StmtKind` | `Return(Box<Expr>)` or `Expr(Box<Expr>)` |
| `Expr` / `ExprKind` | All expression forms; each carries a `SourceSpan` |
| `CascadeMsg` | Unary / Binary / Keyword continuation message |
| `BlockLit` | Params, locals (`Vec<BlockLocal>`), body |
| `BlockLocal` | `name`, `LocalKind` (Data / Var), `init: Expr` |
| `ObjectLit` | Optional annotation, slot list, body |
| `SlotDecl` / `SlotDeclKind` | Data, Var, Arg, Parent, Method |
| `MethodSel` | Unary / Binary / Keyword selector + parameter names |

`BlockLocal` carries an initializer `Expr` because the grammar requires one
(`DataSlotDecl = identifier "=" Expr`). The evaluator executes it at block
activation time.

### Disambiguation rules

**`(` in primary position**: peek one token ahead.
- `(` followed by `Binary("|")` → object literal `(| … |)`
- `(` followed by anything else → parenthesised expression `( Expr )`

**Slot declarations**: the parser looks ahead without consuming to classify
each slot by the tokens that follow the first token:

| First token(s) | Following tokens | Kind |
|---|---|---|
| `Colon` | `Ident` | `ArgSlotDecl` |
| `Ident` | `Binary("<-")` | `VarSlotDecl` |
| `Ident` | `Binary("*")` `Binary("=")` | `ParentSlotDecl` |
| `Ident` | `Binary("=")` `LParen` *(not followed by `\|`)* | `MethodSlotDecl` (unary) |
| `Ident` | `Binary("=")` *(other)* | `DataSlotDecl` |
| `Binary(sel)` | `Ident` `Binary("=")` `LParen` | `MethodSlotDecl` (binary) |
| `Keyword` / `CapKeyword` | … | `MethodSlotDecl` (keyword) |

**`x = (…)` is always a method body.** After `ident "="`, a `(` not
immediately followed by `|` is the method body delimiter, never the start
of a parenthesised data-slot value. Data slots with complex expressions must
omit the outer parens: `x = a + b`, not `x = (a + b)`.

**`(| … |)` as a data slot value** is supported: `x = (| … |)` is a
`DataSlotDecl` whose value is an `ObjectLit`, because `(` followed by `|`
is always an object literal primary, not a method body.

**`*` in parent slots** must be separated from `=` by whitespace. Writing
`p*= val` produces a single `Binary("*=")` token and fails to parse;
`p* = val` produces `Binary("*")` then `Binary("=")` and parses correctly.

**`|` terminates slot lists.** The closing `|` of a slot section is
`Binary("|")`. Slot-value expressions containing `|` as a binary operator
(e.g. `x = a | b`) must be parenthesised; an unparenthesised `|` ends the
slot list early.

Implementation: `Parser` carries a `stop_at_bar: bool` flag. Each slot-value
call site (`parse_keyword` for Data, Var, Parent values and block locals) sets
the flag to `true` before parsing and restores it after. `parse_binary` checks
the flag before consuming a `Binary("|")` token and breaks the loop if set.
`parse_primary` saves and clears the flag on entry, restoring on exit — so
inside any parenthesised expression, block, or nested object literal the `|`
is again available as a binary operator.

**Block slot syntax.** Block parameters and locals live between `| … |`
delimiters: `[| :x. y = 0. | body]`. The short form `[:x | body]` is not
valid in ego's grammar — `[` is not followed by `|`, so there is no slot
section.

### Keyword accumulation

Both `Keyword` and `CapKeyword` tokens are keyword parts and accumulate into
a single selector string and argument list. `a at: 1 Put: 2` is one
`KeywordSend` with selector `"at:Put:"` and two arguments, not two sends.

---

## Lexical environment (Env)

Bindings for local variables and block parameters are stored in a shared,
reference-counted frame so that mutations inside a block are visible in the
enclosing scope and vice versa.

```rust
pub type Env = Rc<RefCell<HashMap<String, ObjectId>>>;
```

`ObjectId` is `Copy`, so no memory-management issues arise from storing it in
the map. `Rc<RefCell<...>>` here wraps only the lookup table, not ego objects
themselves — no cycles, no leaks.

Each activation starts with a fresh `Env`. When a block is created it captures a
clone of the enclosing `Rc` (same underlying `HashMap`), not a copy of the data.
Nested blocks each clone that same `Rc` again; all share one binding table per
activation.

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ActivationId(u64);
```

`ActivationId` is a monotonically incrementing counter stamped when an
activation is entered. Blocks capture the `ActivationId` of the enclosing method
so that `^` can identify its target.

---

## GC

Stop-the-world mark-and-sweep over the arena. Triggered before allocation when
live count exceeds a threshold. No compaction — indices are stable across
collection, which simplifies the evaluator. Stage 3 introduces a moving
collector.

### Root set

The evaluator holds an explicit `RootSet`:

```rust
pub struct RootSet {
    pub lobby:          ObjectId,
    pub nil_id:         ObjectId,
    pub true_id:        ObjectId,
    pub false_id:       ObjectId,
    pub stack_roots:    Vec<ObjectId>,   // pushed/popped by the evaluator
}
```

Before any operation that may trigger GC, the evaluator pushes every live
`ObjectId` it holds in Rust local variables onto `stack_roots` and pops them
afterward. Block `captures` (the `Env`) are walked by following each `ObjectId`
value in the `HashMap`.

### Mark

Walk all roots, following slot values and `Env` frames, setting
`Object::mark = true`. Parent slots are followed like any other slot.

### Sweep

Walk `objects` linearly. For each object:
- If marked: clear the mark bit.
- If unmarked: clear its slots (to drop `String`s and `Rc`s eagerly), push its
  index onto `free`.

### Trigger

Collect when `objects.len() - free.len() > threshold`. Initial threshold: 4096
live objects. Exposed as a compile-time constant for easy tuning. A forced
collection is available via the `_GcCollect` primitive for tests.

---

## Primitive dispatch

### Underscore convention

A selector whose first character is `_` is a primitive call. The evaluator
checks this at dispatch time; no other pipeline stage treats `_` specially. The
lexer already recognises `_` as a valid leading character for identifiers per
the grammar.

### Primitive table

```rust
pub type PrimFn = fn(
    recv:   ObjectId,
    args:   &[ObjectId],
    arena:  &mut Arena,
    roots:  &mut RootSet,
) -> Result<ObjectId, EgoError>;

pub struct PrimitiveTable {
    map: HashMap<&'static str, PrimFn>,
}
```

Registered during bootstrap before `boot.ego` is evaluated. A `_`-prefixed
selector absent from the table signals `primitiveError`, not
`messageNotUnderstood`. This distinction lets `boot.ego` distinguish a typo in a
primitive name from a normal dispatch miss.

### Initial primitive set

| Selector | Arity | Notes |
|---|---|---|
| `_IntAdd:` | 1 | Promotes to `BigInt` on overflow; also accepts a `BigInt` receiver |
| `_IntSub:` | 1 | Same promotion; result normalizes back to `Integer` if it fits |
| `_IntMul:` | 1 | Same promotion |
| `_IntDiv:` | 1 | Signals `zeroDivide`; promotes on the `i64::MIN / -1` overflow case |
| `_IntMod:` | 1 | Signals `zeroDivide` |
| `_IntLt:` | 1 | Returns `true_id` or `false_id` |
| `_IntLe:` | 1 | |
| `_IntGt:` | 1 | |
| `_IntGe:` | 1 | |
| `_IntEq:` | 1 | |
| `_IntNe:` | 1 | |
| `_IntPrintString` | 0 | Returns `StringVal` |
| `_IntAsFloat` | 0 | Coerce to `Float` |
| `_FloatAdd:` | 1 | |
| `_FloatSub:` | 1 | |
| `_FloatMul:` | 1 | |
| `_FloatDiv:` | 1 | Signals `zeroDivide` |
| `_FloatLt:` | 1 | Returns `true_id` or `false_id` |
| `_FloatEq:` | 1 | |
| `_FloatPrintString` | 0 | |
| `_StringConcat:` | 1 | Argument must be `StringVal`; else `primitiveError` |
| `_StringSize` | 0 | Returns integer byte count |
| `_StringPrintString` | 0 | Returns `self` |
| `_BlockValue` | 0 | Activate zero-arg block |
| `_BlockValue:` | 1 | Activate one-arg block |
| `_BlockValue:Value:` | 2 | Activate two-arg block |
| `_ObjectCopy` | 0 | Shallow clone: duplicate slot list, allocate fresh object |
| `_ObjectEq:` | 0 | Identity comparison (`ObjectId` equality) |
| `_ArrayNew:` | 1 | Fresh `n`-element array, all elements `nil`; signals `primitiveError` if `n` is negative or non-integer |
| `_ArrayAt:` | 1 | Element at 1-based index; signals `primitiveError` if out of range |
| `_ArrayAt:Put:` | 2 | Assign element at 1-based index; signals `primitiveError` if out of range |
| `_ArraySize` | 0 | Element count |
| `_ArrayPrintString` | 0 | Returns `StringVal`; renders `Integer`/`Float`/`StringVal`/nested `Array`/`nil`/`true`/`false` elements directly, other kinds as a placeholder (no message dispatch available from a bare `PrimFn`) |
| `_MirrorOf:` | 1 | Return a mirror wrapping the argument |
| `_MirrorSlotNames` | 0 | Receiver is a mirror; returns array of slot-name strings |
| `_MirrorAt:` | 1 | Slot value by name string; signals `error` if absent |
| `_MirrorAt:Put:` | 2 | Assign slot by name string |
| `_MirrorAddSlot:Value:` | 2 | Add a new data slot |
| `_MirrorRemoveSlot:` | 1 | Remove slot by name; signals `error` if absent |
| `_VarSet:` | 1 | Internal setter used by auto-generated `name:` methods |
| `_GcCollect` | 0 | Force collection (test/debug only; never called from boot.ego) |
| `_PrintLine:` | 1 | Write string to stdout followed by newline; returns `nil` |
| `_Print:` | 1 | Write string to stdout, no trailing newline; explicitly flushes (a bare `print!` can otherwise sit unflushed when `main.rs` exits via `std::process::exit` on an error path); returns `nil` |
| `_ErrorSignal:` | 1 | Signal a `primitiveError` with the given message string |

This list grows with substages. `boot.ego` builds the ego-facing API on top —
`+` on an integer delegates to `_IntAdd:`, `ifTrue:False:` on a boolean
evaluates the appropriate block, and so on. `stdout` (`print:`/`println:`/
`show:`/`nl`) is the first `boot.ego` API attached to an already-running
prototype via mirror-based reflection (`(reflect: self) addSlot:Value:` on
the lobby) rather than a Rust-side `bootstrap.rs` lobby binding — see
`design/stdlib.md`'s Console section.

---

## Bootstrap sequence

Executed once at startup, before any user code:

1. Create the arena and root set.
2. Allocate and register **permanent objects** by `ObjectId`:
   - `nil` (plain object, no slots initially)
   - `true`, `false` (plain objects, no slots initially)
   - `integerProto`, `floatProto`, `stringProto`, `blockProto`, `array` (plain;
     `array`'s `new:` method is wired directly on it, not via a shared trait)
3. Register all primitive functions in the primitive table.
4. Allocate the **lobby** object; add data slots for `nil`, `true`, `false`, and
   the five prototypes; add a `reflect:` method slot backed by `_MirrorOf:`.
5. Set the root set's `lobby`, `nil_id`, `true_id`, `false_id` fields.
6. Parse and evaluate `boot.ego` against the lobby. `boot.ego` defines:
   - Trait objects (`booleanTrait`, `integerTrait`, `floatTrait`, `stringTrait`,
     `blockTrait`) with all method slots (`ifTrue:False:`, `+`, `printString`,
     `value`, `whileTrue:`, `on:Do:`, etc.)
   - Parent slots on the permanent prototypes pointing to their traits
   - Derived library objects (`error`, `messageNotUnderstood`, `zeroDivide`, …)
   - Any lobby bindings that aren't permanent objects (e.g. `math`, collections)
7. Hand control to the user: REPL loop, script execution, or `-e` eval.

Boot failures (parse error or exception during `boot.ego` evaluation) are fatal;
the interpreter exits with a diagnostic and code 1.

---

## Evaluator

### Return type

Every evaluator function returns:

```rust
pub type EvalResult = Result<ObjectId, EgoSignal>;

pub enum EgoSignal {
    Err(EgoError),                          // internal/fatal error
    Exception(ObjectId),                    // ego exception in flight
    NonLocalReturn(ActivationId, ObjectId), // ^ from a block
}
```

`NonLocalReturn` propagates up the Rust call stack until it reaches the
activation whose `ActivationId` matches. On match, it is converted to
`Ok(value)`. If the target activation has already returned (the method exited
before the block fired `^`), it is converted to `Exception(badBlockActivation)`.

The evaluator maintains a `HashSet<ActivationId>` of currently live activations.
Before converting `NonLocalReturn` to `Ok`, the evaluator checks this set.

### Activation context

Activations live on the Rust call stack — not heap-allocated. Each recursive
`eval_method` call creates a local:

```rust
struct Activation<'a> {
    id:           ActivationId,
    self_obj:     ObjectId,
    resend_start: Option<ObjectId>, // Some(obj) when inside a resend; lookup starts from obj's parents
    env:          Env,
}
```

`Activation` is not heap-allocated, so its `ActivationId` is registered in the
live-set on entry and removed on exit (including on unwind via `?`).

### Identifier lookup

A bare identifier `foo` in an expression (AST node `ExprKind::Ident`) is
evaluated as follows:

1. Look up `foo` in the current activation's `env` (local variables and block
   parameters). If found, return the value immediately.
2. Otherwise, perform an implicit unary send: `eval_send(self_obj, "foo", &[], …)`.

Step 2 is the correct Self/ego semantic: inside a method body, all bare names
are unary messages to `self`. At the top level `self_obj` is the lobby, so
lobby data slots (`nil`, `true`, `false`, the proto objects, etc.) are found by
the same path without special-casing.

### Local variable assignment (`<-` outside a slot-decl header)

`(| x <- 1 |)` declares an object *slot* named `x`, which is always mutated
via its auto-generated `x:` setter message (see below) — never via `<-`
again. But block locals and, by extension, any bare name already bound in
the current `env` (params, block locals) have no slot and thus no setter to
send. Substage 1.10's canonical block example needs exactly this:

```
[| :x. sum <- 0 | sum <- sum + x. sum]
```

`sum <- sum + x` is a *body* statement, not a slot-decl header — `parse_slot_decl`/
`parse_block_slots` only recognize `identifier "<-" expr` inside `| … |`.
Outside that context, `parser.rs`'s `parse_primary_inner` recognizes the same
token sequence (`Ident` immediately followed by `Binary("<-")`) wherever a
primary is expected and produces a dedicated `ExprKind::Assign { name, value }`
node instead of falling through to an ordinary `BinarySend` — `<-` as a
message selector on a non-identifier receiver (`(a foo) <- b`) is unaffected,
since the check only fires when the receiver token is a bare identifier.
`value` is parsed via the same `parse_keyword()` entry point used for
slot-decl values, so the RHS can be an arbitrary expression.

`eval_expr` evaluates `Assign` by writing straight into `activation.env`
(`env.borrow_mut().insert(name, val)`), unconditionally creating the binding
if absent — no message dispatch, no fallback to a slot on `self`. This is
also how a block's header locals get their *initial* binding: block
activation (`eval_block_call`) evaluates each local's `init` expression and
inserts the result into `captures` the exact same way, so declaration and
later reassignment are the same primitive operation. Because `<-` only
intercepts a bare-identifier LHS, and block/method env tables are otherwise
untouched by ordinary message dispatch, this doesn't collide with the
slot/setter mechanism described in "Var slots and auto-generated setters"
below — that remains the only way to mutate a slot on an object.

Known gap, not enforced: a `Data`-kind local (`sum = 0`, not `sum <- 0`) is
*supposed* to read as a constant, matching the immutability object `Data`
slots get by never having a setter generated. `Assign` doesn't check the
originating `LocalKind`, so a `Data` local can currently be reassigned like a
`Var` one. See `backlog.md`.

### Implicit-receiver binary/keyword sends

`min: 5` alone means `self min: 5`, and `+ 3` alone means `self + 3`
(lang-spec.md §2; lang-grammar.md's `KeywordExpr`/`BinaryExpr` productions
make the leading receiver optional for exactly this reason). This was
flagged as implemented-for-unary-only when substage 1.8 revised the
keyword-message grouping rules ("unary case already works via existing
bare-identifier lookup... deferred to a later substage" — self-notes.md
§11) and stayed unimplemented until substage 1.10 needed it: block bodies
routinely mutate an enclosing object's var slot via an implicit-receiver
setter send (`count: count + 1`, mirroring `i: i + 1` in lang-spec.md §7's
`whileTrue:` example), and without it that pattern was a parse error.

`parse_keyword`/`parse_binary` now check, before attempting to parse a
receiver at all, whether the current token is a `Keyword`/`Binary` selector
with nothing in front of it; if so they synthesize an `ExprKind::Self_` node
as the receiver instead of calling into `parse_binary`/`parse_unary`
(which have no primary case for a bare selector token and would otherwise
fail with "expected expression"). The binary case excludes a `Binary("|")`
when `stop_at_bar` is set, so an empty slot-decl value doesn't get
misread as `self | …`. A bare `CapKeyword` still cannot start a message
(only a plain lowercase `Keyword` triggers the implicit-`self` case), so the
"first keyword part must be lowercase" grouping rule (§2) is preserved.

### Method lookup

Given receiver `recv` and selector `sel`:

1. Scan `recv`'s slot list for a slot whose name matches `sel`.
   - For a `Method` slot: activate the method (create activation, evaluate body).
   - For a `Data` or `Var` slot (zero-arg unary): return the slot value directly.
   - For a `Parent` slot with matching name `sel` (unusual but legal): return value.
2. If not found, collect all `Parent` slots of `recv` and search each recursively,
   depth-first, left-to-right among siblings.
3. If still not found:
   - Selector starts with `_`: signal `primitiveError` ("unknown primitive: `_sel`").
   - Otherwise: signal `messageNotUnderstood`.

`resend` skips step 1 and begins at step 2, using the parent chain of the object
that defined the currently executing method (tracked in `Activation::resend_start`).

### Var slots and auto-generated setters

When the evaluator creates a `Var` slot named `x` on an object, it also installs
a synthetic `Method` slot named `x:` whose body is a single call to
`_VarSet: newValue`. This is done at object-literal evaluation time, not at parse
time, so no AST nodes are generated for the setter — it is backed directly by the
`_VarSet:` primitive.

### Cascades

The parser produces a `Cascade` AST node holding a receiver expression and a
list of message sends, where `recv` is the *true* shared receiver (e.g. `a`
in `a foo; bar`) and `msgs` includes every cascaded message, including the
one that appeared before the first `;` (`foo`, then `bar`).

Getting there requires a small rewrite: ordinary expression parsing (parsing
`a foo` before the `;` is even seen) naturally produces the full message send
`UnarySend { recv: a, sel: "foo" }` as one node. On encountering `;`, the
parser peels the outermost send off that node — `split_cascade_head` in
`parser.rs` — recovering `a` as the true receiver and turning the peeled-off
send into the first `CascadeMsg`. Without this step the evaluator would pin
the *result* of `a foo` as the receiver of `bar`, which is wrong (see
lang-spec.md §9).

The evaluator itself stays simple given the corrected AST:

1. Evaluates the receiver expression once and pins the `ObjectId`.
2. Sends each message in the list to that pinned receiver in order.
3. Returns the result of the last message send.

### Non-local return across block boundaries

`^` inside a block compiles to an AST node `NonLocalReturn(expr)`. The evaluator:

1. Evaluates `expr` → `val`.
2. Raises `EgoSignal::NonLocalReturn(home_id, val)`.
3. Each call frame checks on the way out: if `NonLocalReturn.0 == self.id`,
   convert to `Ok(val)`. If the `home_id` is not in the live-set, convert to
   `Exception(badBlockActivation)`.

### Exception handling

`[body] on: ExceptionType Do: [:e | handler]` is a keyword message sent to a
block. The evaluator intercepts this at method-slot lookup on block objects — it
is not a special form. The `on:Do:` method slot in `boot.ego` delegates to a
`_BlockOnDo:Do:` primitive that:

1. Evaluates the protected block.
2. On `EgoSignal::Exception(exc)`: checks if `exc` matches `ExceptionType` (by
   walking the exception object's parent chain looking for an object `===`
   `ExceptionType`).
3. If matching: activates the handler block with `exc` as its argument.
4. If not matching: re-raises.

Handler operations (`e return`, `e resume:`, `e retry`, `e outer`, `e signal`)
are methods on exception objects defined in `boot.ego`, backed by primitives for
the operations that require evaluator cooperation (`_ExceptionResume:`,
`_ExceptionRetry`, `_ExceptionOuter`).

---

## REPL and CLI

Follows [cli.md](cli.md). Three modes:

| Mode | Invocation | Prints result? |
|---|---|---|
| REPL | `ego --repl` | Yes — each expression's `printString` |
| Script | `ego file.ego` | No — program prints explicitly |
| Inline eval | `ego -e 'expr'` | Yes — final expression's `printString` |
| Mixed | `ego -e 'x<-1' file.ego` | No (script rule applies) |

Running `ego` with no arguments is exit code 2 (bad arguments).

**REPL multi-line input:** Track nesting depth of `(`, `[`, and open strings
(`'`). Continue reading lines until depth reaches zero. This is simpler than
re-entering the parser on partial input and sufficient for the likely case of
multi-line blocks and object literals.

**REPL error recovery:** On `EgoSignal::Exception`, print the exception's
`messageText` to stderr and continue. On `EgoSignal::Err`, print and continue.
Never exit the REPL on a runtime error.

---

## Error reporting

Source locations are attached to every token and propagated through the AST. The
`SourceSpan` type:

```rust
pub struct SourceSpan {
    pub file:   Rc<String>,  // file path or "<repl>" or "<eval>"
    pub line:   u32,
    pub column: u32,
}
```

`EgoError` carries a `SourceSpan` and a message string. Printed to stderr:

```
path/to/file.ego:12:5: error: message not understood: foo
<repl>:1:3: error: ...
<eval>:1:1: error: ...
```

Line and column are 1-based. For stack-like exception unwinding, the outermost
`on:Do:` handler or the top-level runner prints the span; intermediate frames do
not print.

### Placeholder spans (primitives and bootstrap-synthesized methods)

Two constructs have no real source text of their own, so they cannot carry a
meaningful `SourceSpan`, and each stamps a placeholder instead:

- `PrimFn` (`primitives.rs`) only threads `Arena`/`RootSet`, not a span, so
  errors raised inside a primitive use `prim_span()`, a dummy `<primitive>:0:0`.
- Bootstrap-synthesized methods (`+`, `/`, and the other operator-to-primitive
  forwarders built by `make_unary_prim_method`/`make_binary_prim_method` in
  `bootstrap.rs`, not parsed from `boot.ego` text) carry a dummy
  `<bootstrap>:0:0` span on every AST node in their one-statement body.

Both placeholders are rewritten to the real call-site span at the one point
that has it:

- `eval_send`'s primitive-dispatch branch unconditionally overwrites any
  error's span with the span it was called with — a primitive's own span is
  never meaningful, so this is safe unconditionally.
- `eval_method` rewrites an error's span only when the erroring statement's
  file is exactly `"<bootstrap>"`, substituting its own `span` parameter (the
  real call site). A genuine user-defined method is left alone: its body has
  real per-statement spans from parsed source, and an error inside it should
  point inside the method, not at the call site — this only fires for the
  synthetic forwarders.

Without this, `1 / 0` would report `<bootstrap>:0:0: error: division by zero`
instead of the user's actual file, line, and column.

---

## Testing

### Framework

Standard `cargo test`. Use `rstest` for table-driven tests where the parameter
list makes `#[test]` repetition unreadable. No other testing crate dependency.

### Lexer tests (substage 1.1)

Table-driven in `tests/lexer_tests.rs`. Each row: source string → expected
`Vec<Token>`. Cover every token kind, every edge case in the grammar (based
integers, float exponents, `''` in strings, `""` in comments, `_` identifiers).
These tests never change after the grammar is finalised.

### Parser tests (substage 1.2)

Table-driven in `tests/parser_tests.rs`. Each row: source string → expected AST
(compared via `Debug` or `PartialEq` derives). Cover every grammar production,
including error cases (malformed input → expected error type). Parser tests are
independent of the evaluator and remain valid across all later stages.

### GC tests (substage 1.4)

Unit tests in `tests/` (or inline in `gc.rs`). Allocate objects, drop root
references, force collection via `_GcCollect`, assert that dead objects are
reclaimed and live objects are intact. These tests may inspect arena internals
directly (white-box). Cover at minimum: cycles, objects referenced only through
parent chains, blocks capturing variables.

### Evaluator golden tests (substages 1.5–1.18)

Each test: a `.ego` file in `tests/eval_golden/` plus a `.expected` file. The
harness in `tests/eval_golden.rs` runs each `.ego` file through the interpreter,
captures stdout, and compares it to the `.expected` file line-by-line.

One growing suite, organised by substage in subdirectories:

```
tests/eval_golden/
  1.5-literals/
  1.6-objects/
  1.7-var-slots/
  ...
  1.16-arrays/
  1.17-mirrors/
  1.18-bignums/
```

This suite is the cross-stage regression guard. Stage 2 (bytecode VM) and
Stage 3 (Zig VM) run identical inputs and must produce identical output.

**Windows CRLF gotcha:** `core.autocrlf` converts checked-in `.ego`/`.expected`
fixtures to CRLF on checkout. The lexer tolerates `\r` fine, but a naive
trailing-line comparison in the harness doesn't. Compare captured output and
the `.expected` file using `.trim_end()`, not `.trim_end_matches('\n')`, so a
trailing `\r` doesn't break exact-string golden comparisons.
