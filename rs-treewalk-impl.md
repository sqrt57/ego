# Stage 1: Rust Tree-Walking Interpreter

Design document for the Rust tree-walking interpreter. Covers crate layout,
object model, GC, primitive dispatch, bootstrap, evaluator, REPL, diagnostics,
and testing. Corresponds to ROADMAP substages 1.1–1.17.

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
    BigInt(Box<BigInt>),        // num-bigint crate; added at substage 1.17
    StringVal(Box<str>),
    Method(Rc<MethodDef>),
    Block(Box<BlockData>),
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
    pub params:          Vec<String>,
    pub locals:          Vec<(String, SlotKind)>,   // SlotKind ∈ {Data, Var}
    pub body:            Rc<Vec<AstNode>>,
    pub home_id:         ActivationId,   // for non-local ^ return
    pub captured_self:   ObjectId,
    pub captured_resend: Option<ObjectId>,
    pub captures:        Env,            // shared lexical frame (see §Env)
}
```

Blocks close over `self`, `resend` (if inside a method), and all bindings
visible in the enclosing scope at the point the block literal is evaluated.
`home_id` is the `ActivationId` of the enclosing method; `^` inside the block
targets this activation.

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
| `_IntAdd:` | 1 | Promotes to BigInt on overflow (substage 1.17) |
| `_IntSub:` | 1 | |
| `_IntMul:` | 1 | |
| `_IntDiv:` | 1 | Signals `zeroDivide` |
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
| `_MirrorOf:` | 1 | Return a mirror wrapping the argument |
| `_MirrorSlotNames` | 0 | Receiver is a mirror; returns array of slot-name strings |
| `_MirrorAt:` | 1 | Slot value by name string; signals `error` if absent |
| `_MirrorAt:Put:` | 2 | Assign slot by name string |
| `_MirrorAddSlot:Value:` | 2 | Add a new data slot |
| `_MirrorRemoveSlot:` | 1 | Remove slot by name; signals `error` if absent |
| `_VarSet:` | 1 | Internal setter used by auto-generated `name:` methods |
| `_GcCollect` | 0 | Force collection (test/debug only; never called from boot.ego) |
| `_PrintLine:` | 1 | Write string to stdout followed by newline; returns `nil` |
| `_ErrorSignal:` | 1 | Signal a `primitiveError` with the given message string |

This list grows with substages. `boot.ego` builds the ego-facing API on top —
`+` on an integer delegates to `_IntAdd:`, `ifTrue:ifFalse:` on a boolean
evaluates the appropriate block, and so on.

---

## Bootstrap sequence

Executed once at startup, before any user code:

1. Create the arena and root set.
2. Allocate and register **permanent objects** by `ObjectId`:
   - `nil` (plain object, no slots initially)
   - `true`, `false` (plain objects, no slots initially)
   - `integerProto`, `floatProto`, `stringProto`, `blockProto` (plain)
3. Register all primitive functions in the primitive table.
4. Allocate the **lobby** object; add data slots for `nil`, `true`, `false`, and
   the four prototypes; add a `reflect:` method slot backed by `_MirrorOf:`.
5. Set the root set's `lobby`, `nil_id`, `true_id`, `false_id` fields.
6. Parse and evaluate `boot.ego` against the lobby. `boot.ego` defines:
   - Trait objects (`booleanTrait`, `integerTrait`, `floatTrait`, `stringTrait`,
     `blockTrait`) with all method slots (`ifTrue:ifFalse:`, `+`, `printString`,
     `value`, `whileTrue:`, `on:do:`, etc.)
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
list of message sends. The evaluator:

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

`[body] on: ExceptionType do: [:e | handler]` is a keyword message sent to a
block. The evaluator intercepts this at method-slot lookup on block objects — it
is not a special form. The `on:do:` method slot in `boot.ego` delegates to a
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
`on:do:` handler or the top-level runner prints the span; intermediate frames do
not print.

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

### Evaluator golden tests (substages 1.5–1.17)

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
  1.17-bignums/
```

This suite is the cross-stage regression guard. Stage 2 (bytecode VM) and
Stage 3 (Zig VM) run identical inputs and must produce identical output.
