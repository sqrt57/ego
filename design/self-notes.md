# Self Language Notes

Reference: Self Handbook 2024.1 (https://handbook.selflanguage.org/2024.1/).
Supporting papers listed in [references.md](references.md).

These notes extract the decisions most relevant to ego design. Each section
closes with an **ego stance**: adopt, adapt, or diverge — and why.

---

## 1. Core Philosophy

Self's central claim ("The Power of Simplicity", Ungar & Smith 1987): eliminate
the class/instance distinction. Objects exist directly — no blueprint, no factory.
Every object can hold both data and methods in the same slot namespace. Behavior
is shared through *delegation* via parent slots, not through class hierarchies.

Three simplifications relative to Smalltalk-80:

1. **No classes** — cloning replaces instantiation; delegation replaces
   class-based lookup.
2. **No global variables** — the *lobby* is the single root object; all names
   resolve through it or the receiver.
3. **No special syntax for control flow** — `if`, loops, and exception handling
   are ordinary message sends.

**Ego stance — adopt.** All three simplifications carry over directly. Ego's
philosophy section in `lang-spec.md` reflects them.

---

## 2. Object Model and Slot Taxonomy

An object is an unordered collection of named **slots**. Self defines five kinds:

| Kind | Syntax | Semantics |
|---|---|---|
| Data | `name = expr` | Constant value; sends `name` return it |
| Assignable | `name <- expr` | Mutable; auto-generates `name:` setter |
| Parent | `name* = expr` | Value is a parent; delegates failed lookups |
| Argument | `:name` | Method parameter; only in method objects |
| Method | `name = ( stmts )` | Code executed on message send |

Method slots match the three message kinds:

```
"unary"   abs = ( self < 0 ifTrue: [0 - self] False: [self] ).
"binary"  + n = ( primitiveAdd: n ).
"keyword" at: key Put: val = ( ... ).
```

`self` inside a method refers to the original message receiver (not the object
holding the method slot). This is the standard dynamic self — the same as
Smalltalk's `self`, not a lexically bound name.

`resend` is a pseudo-variable that restarts lookup from the current object's
parent, enabling a delegating call to an "overridden" method.

**Ego stance — adopt.** All five slot kinds. `self` and `resend` semantics
match exactly. Only the syntax for argument slots differs — ego uses `:name`
inside block headers rather than as standalone slot declarations in object
literals, because ego does not use the Self standalone-method-object idiom.

### Object literal construction: two-phase, lobby-scoped initializers

Source: Self Handbook 2024.1 §3.1.9 ("Construction of object literals").

Object literals are built in two phases:

1. **Slot initializers** (the `= expr` / `name* = expr` right-hand sides)
   evaluate once, at construction time, in the context of the **lobby** —
   not the lexical context of whatever method the literal appears inside,
   since no such activation exists yet when the literal is being built.
   Quote: "Slot initializers are *not* evaluated in the lexical context,
   since none exists at parse time; they are evaluated in the context of
   an object known as the `lobby`." A slot initializer therefore cannot
   reference another slot of the object being constructed (no `letrec`)
   and cannot see the enclosing method's `self` or temporaries — only
   what's reachable from the lobby.
2. **Method-slot bodies** run later, at invocation time, with `self` bound
   to the constructed object itself. They have no lexical link back to
   whatever code built the literal.

Net effect: a nested object literal's method body has no ambient path to
the enclosing method's bindings, nor (in general) to the lobby's globals —
its only reachable state is its own slots and whatever its parent chain
resolves to. The idiomatic workaround is to capture a needed lobby/outer
binding into a data slot via the initializer (which *can* see the lobby),
then read that slot from the method body (an ordinary local self-send).
Blocks (§5) are the sole exception — they carry a genuine lexical link.

**Ego stance — adopt.** Spec section added: `lang-spec.md` §1, "Two-phase
construction."

---

## 3. Prototype / Traits Split

Self programs use a consistent two-object idiom for shared behavior:

- **Prototype object** (`point`) — holds the data slots (`x`, `y`), serves as
  the template for cloning. Holds a parent slot pointing to its traits object.
- **Traits object** (`point traits`) — holds all method slots. Never cloned
  directly; shared by all point instances as their parent.

Cloning a prototype copies only its data slots; all clones share the same
traits parent by reference. This achieves the class/instance contract (shared
methods, per-instance state) without introducing a class primitive.

The "Organizing Programs Without Classes" paper (1991) codifies this as a
recommended idiom, not a language requirement. The language does not enforce it.

**Ego stance — adopt as idiom, not spec.** The lang-spec will not mandate
the split, but standard-library objects (booleans, numbers, strings,
collections) will follow it. A future style guide can recommend it to users.

---

## 4. Message Dispatch and Lookup Algorithm

Lookup proceeds as follows for a message `m` sent to receiver `R`:

1. Search `R`'s own slots for a slot named `m`.
2. If not found, recursively search each parent slot's value, depth-first,
   left to right.
3. If found in exactly one place, invoke it.
4. If found in more than one place (ambiguity), signal a **message-not-understood**
   with an ambiguity report.
5. If not found anywhere, signal **message-not-understood**.

Multiple parent slots are legal and common. Ambiguity is a runtime error, not
a compile-time one. Programs avoid it by design (the traits split helps: only
one path to methods).

**Ego stance — adopt.** Same lookup algorithm, same ambiguity rule. The
arena-based object model in Stage 1–2 will implement this directly; Stage 3
(Zig VM) may add inline caches for the common one-parent case.

---

## 5. Blocks

A block is a closure literal:

```
[| :x. sum <- 0 | sum <- sum + x. sum]
```

Between the `|` delimiters: parameter declarations (`:name`) and local
variable declarations (`name <- init`). Parameters are positional. The body
follows the closing `|`.

Blocks are invoked by sending `value` (0 params), `value:` (1), `value:With:`
(2), `value:With:With:` (3), etc. — ordinary messages, no special call
syntax. The repeated part is capitalized (`With:`) rather than `value:value:`
because of the keyword-message capitalization rule (§11).

**Non-local return** — `^` inside a block returns from the *enclosing method*,
not just from the block. This is the Smalltalk-80 convention. A block whose
enclosing activation has already exited is a **dead block**; a `^` inside it
that fires after that point signals an error (`badBlockActivation`).

**Activation objects and the lexical chain.** Source: Self Handbook 2024.1
§3.1.6 (Methods), §3.1.7 (Blocks), §3.3.4 (implicit-receiver messages). A
block literal creates two runtime objects: a *block data object* (parent →
shared block behavior) and a *block method object*. Unlike an ordinary
method object, the block method object has no `self` slot — instead it has
an anonymous parent slot initialized to the **activation object of the
lexically enclosing block or method**. This anonymous parent slot is the
actual closure mechanism: nested blocks chain outward through activation
objects until reaching the outermost method activation.

An ordinary (non-block) method activation object, by contrast, has only a
`self` parent slot, initialized to the message **receiver** — no lexical
parent at all: "The clone's self parent slot is initialized to the
receiver of the message." Implicit-receiver message lookup begins at the
*current activation object*, not the receiver directly; for a block this
walks the activation chain outward before falling through to `self`'s own
parent chain — which is what lets a block reference an enclosing method's
locals and `self` even many levels of nesting deep. Object literals get
none of this (§2's two-phase-construction note): their method bodies are
`self`-rooted only, with no activation-chain fallback.

**Ego stance — adapt.** Same block syntax, same `value`/`value:With:`
activation, same `^` non-local-return semantics. Narrower than classic
Smalltalk-80's blanket "sending `value` to a dead block is always an error":
Smalltalk-80 stack-allocates method contexts, so a dead block's captured
state is genuinely gone once the context is popped/reused. Ego's `Env` is
always heap-allocated and GC-tracked, so a dead block that never executes
`^` remains safe to invoke indefinitely — required for the standard
"closure factory" pattern (a method returning a block that mutates a var
slot it closed over). Only an actual `^` targeting a dead activation is an
error; see `rs-treewalk-impl.md`'s block-activation section.

---

## 6. The Lobby

Top-level expressions evaluate in the context of the **lobby** — the root
object. The lobby holds slots for all globally accessible objects: `true`,
`false`, `nil`, number and string prototypes, collection prototypes, and
user-defined globals. There is no separate global namespace or module system.

REPL interaction sends each typed expression to the lobby as its receiver.

**The lobby is not a universal ancestor.** Source: `worldorg.html`,
`roots.html`. The relationship runs the other way: the lobby's `globals`
slot is itself a *parent* slot, so the lobby inherits from
`globals`/`traits`, not vice versa — "The `globals` slot... is a parent
slot, so the name of a prototype object needs no prefix." Nothing wires
the lobby into the parent chain of arbitrary receivers elsewhere in the
system.

There is also no special syntax or compiler mechanism for a method body to
reach globals — `langref.html` §3.4.2 lists only `self` and `resend` as
reserved identifiers; even `true`/`false`/`nil` resolve via ordinary
implicit-receiver sends. In practice, method bodies reach globals only
because most objects in the system are conventionally built to inherit the
lobby's `defaultBehavior` (identity comparison, printing, mirror creation,
etc.) — explicitly opt-out, not opt-in: "It is entirely permissible to
construct objects that do not inherit from the lobby." An object whose
parent chain was never wired back toward `defaultBehavior`/`globals` — a
bespoke nested object literal, for instance — has no path to the lobby at
all.

**Ego stance — adopt, with the reachability question resolved.** ego's
lobby is the same concept: global state is accessible only through it,
there are no implicit global variables. Unlike Self, where reaching the
lobby from an arbitrary object is convention that individual traits either
opt into or don't, ego **enforces** it for the standard library: every
built-in trait (`int_trait`, `float_trait`, `string_trait`, `block_trait`,
`bool_trait`, `exception_trait`) is wired in `bootstrap.rs` with a
`parent*` slot straight to the lobby, so anything that clones from a
stdlib prototype is guaranteed a path to `true`/`false`/`nil`/other
globals. This was a real gap, not just theoretical: before this wiring
existed, no non-top-level method body could reach a lobby global at all —
see `treewalk/tests/eval_golden/1.6-objects/builtin_trait_reaches_lobby.ego`.
A genuinely bespoke object with no parent slot of its own — a nested object
literal with no `parent* = ...` clause, say — still gets none of this for
free, matching Self's opt-out behavior exactly; the enforcement is scoped
to "built from the standard library," not universal.

---

## 7. Cascades

The `;` operator sends subsequent messages to the *same receiver* as the
preceding message, without repeating the receiver expression:

```
collection add: 1; add: 2; add: 3.
```

is equivalent to:

```
collection add: 1.
collection add: 2.
collection add: 3.
```

The result of a cascade expression is the result of the *last* message in
the chain. The receiver is the value of the expression *before* the first `;`,
re-evaluated once — not once per message.

Cascades work with all three message kinds:

```
stream nextPutAll: 'hello'; nl; close.
```

**Ego stance — adopt.** Same semantics. Spec section goes in `lang-spec.md`
§ Cascades (pending). Grammar already includes `;`; substage 1.14 implements it.

---

## 8. Mirrors

Source: Bracha & Ungar, "Mirrors: Design Principles for Meta-level Facilities",
OOPSLA 2004. Self 4.x implements the mirror API.

### Problem mirrors solve

Naive reflection attaches introspective methods directly to every object
(`respondsTo:`, `perform:`, `instVarAt:`, `class`, etc.). This:

- Pollutes every object's namespace with rarely-needed methods.
- Creates security issues (reflection bypasses encapsulation).
- Makes the core object model more complex.

### Mirror API

In Self, obtain a mirror by sending `reflect:` to the lobby:

```
| m |
m: reflect: anObject.
m slotNames.           "returns array of slot names"
m at: 'x'.            "returns the value of slot named 'x'"
m at: 'x' Put: 42.    "assigns slot 'x'"
m addSlot: 'z' = 0.   "adds a new slot"
m removeSlot: 'x'.    "removes slot 'x'"
```

A mirror is an ordinary Self object. It holds a reference to the reflectee.
The mirror's slots are the introspection API; the reflectee's slots are
untouched.

Key design principles (from the paper):

1. **Encapsulation** — reflection is not automatically available; you must
   have the mirror object, which can be withheld.
2. **Stratification** — the mirror API and the base object API are separate
   namespaces; no name collisions possible.
3. **Ontological correspondence** — each language concept has a corresponding
   mirror concept (slot → slot mirror, method → method mirror, etc.).

### In Self 4.x

The full mirror hierarchy:

- `mirror` — base; wraps any object
- `slotMirror` — represents a single slot; provides name, value, kind
- `methodMirror` — wraps a method slot; can decompile

Mirrors are used internally by the Self IDE (the Morphic browser), which is
how objects are inspected and edited live.

**Ego stance — adopt API shape, simplify for Stage 1.**

- `reflect: anObject` returns a mirror object (via lobby).
- Mirror provides: `slotNames`, `at:`, `at:Put:`, `addSlot:Value:`, `removeSlot:`.
- No slot/method sub-mirrors in Stage 1 — a flat API is sufficient.
- Richer sub-mirror hierarchy deferred to later stages if needed.
- The stratification principle is the load-bearing constraint: base objects
  must have no reflective methods (`respondsTo:` etc. are not on `object traits`).

Spec section goes in `lang-spec.md` §11 Mirrors (done). Precedes substage
1.17 — bumped from 1.16 once arrays (substage 1.16) turned out to be a
prerequisite: `slotNames` returns an array, and nothing before mirrors
introduced one.

---

## 9. Exception Handling

Self's exception handling is entirely message-based, consistent with the
"no special syntax" principle.

### Raising an exception

Any object can be used as an exception prototype. Signalling:

```
anException signal.
anException signal: 'message text'.
```

`signal` is an ordinary method on exception prototype objects. It unwinds the
stack looking for a handler.

### Catching an exception

```
[risky code] on: ExceptionType Do: [:e | handler].
```

`on:Do:` is a keyword method on blocks. It evaluates the receiver block; if
an exception of `ExceptionType` (or a subtype via parent chain) is signalled,
the handler block is invoked with the exception object as its argument.

`ExceptionType` can be a single exception prototype or an `ExceptionSet`
(obtained via `|` — `Error | ZeroDivide`).

### Handler options

Inside the handler block, `e` is the exception object. Available messages:

| Message | Effect |
|---|---|
| `e return` / `e return: val` | Exits `on:Do:` expression, returning nil / val |
| `e retry` | Re-executes the protected block from the start |
| `e resume` / `e resume: val` | Resumes execution after the `signal` send |
| `e outer` | Passes to the next outer handler for the same type |
| `e signal` | Re-raises the exception |
| `e messageText` | Returns the exception's description string |

The default behaviour (if the handler exits normally) is equivalent to
`e return:` with the handler's value.

### Exception hierarchy

Exception types are ordinary prototype objects linked via parent slots:

```
error → primitiveError (arithmetic, etc.)
      → messageNotUnderstood
      → userDefinedError (user subclasses)
```

Catching a parent type catches all subtypes, because `on:Do:` checks the
parent chain of the signalled exception against `ExceptionType`.

### Built-in exceptions in Self

- `error` — base for all exceptions
- `primitiveFailure` — failed primitive operation
- `messageNotUnderstood` — no method found; carries receiver and message name
- `badBlockActivation` — dead non-local return attempted

**Ego stance — adopt.**

- `[...] on: E Do: [:e | ...]` syntax and semantics: adopt (capitalization
  per the keyword-grouping rule in §11 — not literally verified against
  Self's own library naming, but required for ego's own grammar).
- Exception types as prototype objects (parent chain = type hierarchy): adopt.
- Handler messages (`return`, `retry`, `resume`, `outer`, `signal`): adopt.
- Built-in exceptions: `error`, `messageNotUnderstood`, `badBlockActivation`,
  arithmetic errors — adopt. `primitiveFailure` renamed to `primitiveError`
  for clarity (TBD at spec time).
- `ExceptionSet` via `|`: defer — not needed for Stage 1.

Spec section goes in `lang-spec.md` § Exception Handling (pending; must precede
substage 1.15).

---

## 10. Numeric Tower

Self's numeric objects:

| Type | Precision | Notes |
|---|---|---|
| `smallInt` | Machine word (30-bit on 32-bit Self) | Tagged pointer, no heap allocation |
| `largeInt` (bignum) | Arbitrary precision | Promotes automatically on overflow |
| `float` | IEEE 754 double | |

Mixed arithmetic: integer + float → float. Integer arithmetic overflows
silently into bignum — no explicit promotion needed from user code.

Character literals (`$A`) are distinct from integers in Self but support
arithmetic.

**Ego stance — adopt tower, simplify character handling.**

- `integer` (machine word) → auto-promotes to bignum on overflow: adopt.
- `float` (IEEE 754 double): adopt.
- Mixed integer/float → float: adopt.
- Character literals (`$A`): ego uses character syntax but characters are
  not a separate numeric type — they are integers (Unicode codepoints). This
  is a deliberate divergence; Self's character object design is heavier than
  needed.

---

## 11. Message Syntax and Precedence

Source: Self Handbook 2024.1, §3.3 (Expressions) and §3.4.5 (Operators).

### Precedence and binary-operator associativity

Precedence, tightest first: unary > binary > keyword — same as Smalltalk-80.
But binary messages **have no associativity except between identical
operators**: `3 + 4 + 7` parses as `(3 + 4) + 7`, while `3 + 4 * 7` is
illegal and must be parenthesized. Smalltalk-80 allows any mix of binary
operators to associate left-to-right (`3 + 4 * 2` = 14); Self deliberately
closes off that footgun.

### Keyword-message grouping via capitalization

A keyword message's first part must be lowercase-initial (or `_`); a
cap-initial part *continues* the message in progress, but a lowercase-initial
part *closes* it and starts a new, nested message — right-associatively,
with no parentheses needed:

```
5 min: 6 min: 7 Max: 8 Max: 9 min: 10 Max: 11
"= 5 min: (6 min: 7 Max: 8 Max: (9 min: 10 Max: 11))"
```

This is a real constraint on naming, not just a style convention: any
multi-part keyword selector where the parts should concatenate into one
message *must* capitalize every part after the first. Self's own library
reflects this — `ifTrue:False:` (not `ifTrue:ifFalse:`), `value:With:` for
two-arg block activation (not `value:value:`).

### Implicit-receiver messages

Unary, binary, and keyword messages can all be sent with the receiver
omitted, meaning "send to `self`": `factorial`, `+ 3`, `max: 5`. Lookup for
these begins at the *current activation* (locals/params first), not at the
receiver directly — this is how assignable-slot access reads (`t`) and
writes (`t: 17`) without writing `self` everywhere. Explicitly sending to
`self` is considered bad style in Self.

### Resend syntax

`resend` is not an ordinary object sent ordinary messages — it's special,
whitespace-sensitive syntax: `resend.display`, `resend.min: 17 Max: 23` (no
space around the `.`). Self additionally supports **directed resend**:
naming a specific parent slot instead of `resend` (`intParent.min: 17`)
constrains the lookup to that one parent, resolving the case where a method
is reachable through more than one parent.

**Ego stance — adopt all of the above**, with the naming fallout that
implies: every ego selector with a lowercase-initial part after the first
had to be renamed to capitalize it (`on:do:` → `on:Do:`, `ifTrue:ifFalse:` →
`ifTrue:False:`, `value:value:` → `value:With:`, `between:and:` →
`between:And:`). The binary-operator restriction and keyword-grouping rule
are implemented in `treewalk/src/parser.rs` (`parse_binary`,
`parse_keyword_chain`). The resend dot-syntax (undirected and directed) was
implemented at substage 1.9 — see the ROADMAP entry for details; the tight
(no-whitespace) `.` is resolved at the lexer level (`Token::ResendDot` in
`lexer.rs`), not via lookahead in the parser. Implicit-receiver binary/
keyword sends were implemented at substage 1.10, once block bodies started
needing them (`count: count + 1` inside a block, mutating an enclosing
object's var slot) — see `rs-treewalk-impl.md`'s "Implicit-receiver
binary/keyword sends" section (`parse_keyword`/`parse_binary` in
`parser.rs`).

Built-in numeric/string prototypes give their instances a parent slot named
`"parent"` (a producible `identifier`, reachable via `parent.selector`
directed resend). Earlier this slot was hardcoded in Rust as the literal
`"parent*"` (asterisk baked into the stored name), which never matched — the
parser strips the trailing `*` when it reads `ParentSlotDecl` from ego
source, so any user-written `parent* = X` was already stored as bare
`"parent"`; the Rust-constructed built-in slots just hadn't followed the
same convention. Fixed post-1.17 (see `backlog.md`) by renaming every
hardcoded built-in parent-slot name from `"parent*"` to `"parent"` across
`bootstrap.rs`/`gc.rs`/`primitives.rs`/`eval.rs`.

Directed resend only earns its keep when a selector is reachable through
*more than one* parent, and the built-in chain (`3 → integer_proto →
int_trait`, one parent per link) is strictly linear — undirected `resend`
already reaches the whole chain either way. `_MirrorAddSlot:Value:`
(substage 1.17) always creates a `SlotKind::Data` slot, never
`SlotKind::Parent` (`primitives.rs`), so mirrors cannot yet attach a second
*real* parent to a built-in proto — the multi-parent-ambiguity scenario
remains theoretical, not live. Revisit if `addSlot:Value:` (or a future
mirror primitive) is extended to create genuine parent-kind slots.

---

## 12. World Objects Survey (Remaining Ch. 4 Sections)

Source: Self Handbook 2024.1, Chapter 4 ("The Self World"), sections not
covered by a dedicated section above. Recorded here as reference for future
design work; stances marked **TBD** are not yet decided and should not be
treated as settled.

### Pairs (§4.6)

`traits pair` describes general arithmetic-pair behavior; `point` (2D
coordinate) and `rectangle` (two opposing axis-aligned corners) are its two
concrete uses. **Status: TBD** — not in `stdlib.md`'s current scope.

### Collections, beyond what's in `stdlib.md` (§4.5)

Self's own collection hierarchy is broader than ego's planned `array` /
`orderedCollection` / `dictionary` / `set`: hash-based `sharedSet` /
`sharedDictionary` (locking variants), sorted `treeSet` / `treeBag` (dynamic
inheritance distinguishes empty/non-empty subtrees; degrades if fed
pre-sorted input), circular doubly-linked `list`, `priorityQueue`, and a
`collector` object that builds collections via the `&` operator (`(1 & 2 &
3) asList`). **Status: TBD beyond what `stdlib.md` already specifies** — no
plan yet for sorted/tree collections, priority queues, or `&`-based
construction syntax (ego has no such operator).

### Processes and Concurrency (§4.9)

`semaphore`, `barrier`, `lock` primitives; a `channel` object wraps a target
with locking and exposes `waitingInbox` (blocks until available),
`waitingInboxTimeOut:IfTimedOut:`, non-blocking `inbox`, and
`inboxTimeOut:`. `prompt` reads stdin and spawns one process per line.
**Status: out of scope** — `ROADMAP.md` marks concurrency out of scope for
the foreseeable stages; recorded here only for future reference if that
changes.

### Foreign Objects / FFI (§4.10)

Proxies (`proxy`, `fctProxy`) wrap foreign pointers with validity metadata so
snapshots can detect stale references after restore on a different machine;
`foreignFct`/`foreignCode`/`foreignCodeDB` provide the ego-facing (well,
Self-facing) API on top, with direct proxy manipulation discouraged.
**Status: deferred** — `stdlib.md` already lists FFI under "Deferred."

### I/O and Unix (§4.11)

Self's own docs call this section outdated (pre-4.5) and defer to an `os`
object. Covers raw syscalls (`creat`, `open`, `read`, `write`, `lseek`,
`unlink`), `tcpConnectToHost:Port:IfFail:`, `select()` multiplexing, and a
`prompt suspendWhile: [...]` idiom to stop the REPL prompt from stealing
stdin. **Status: deferred** — networking is out of scope per `stdlib.md`;
ego's own console/file I/O design already lives in `stdlib.md` and doesn't
need Self's raw-syscall layer.

### Miscellaneous Oddball Objects (§4.12)

Singletons with no ego equivalent yet: `comparator` (sequence diffs),
`desktop` (GUI controller — out of scope, see below), `memory` (GC/snapshot
introspection), `monitor`, `preferences`, `thisHost`, `typeSizes`,
`vmProfiling`. **Status: TBD**, mostly low priority; most are either
GUI-adjacent (out of scope) or VM-introspection conveniences with no current
ego use case.

### Low-Level Interrupts and Textual Debugger (§4.13, §4.14)

Control-C / Control-`\` interrupt a running process into an interactive menu
(kill/background/suspend/stack-trace); the textual debugger supports
`attach:`/`detach`/`cont`, stepping (`step`/`stepi`/`next`/`nexti`/`finish`),
stack navigation (`trace`/`show`/`up`/`down`/`upLex`), and `lookup:`.
**Status: TBD** — no ego debugger design exists yet; worth revisiting once
the VM stages (2–3) are underway, since a textual debugger is much cheaper
to build than a GUI one and Self's command set is a reasonable starting
point.

### Logging (§4.15)

Self's `log` object (levels `debug`/`info`/`warn`/`error`/`fatal`, deferred
block-valued messages, `log dispatcher`/`log prototypeHandlers` for custom
sinks). **Status: adopted, simplified** — ego's version is specified in
`stdlib.md` § Logging; handler registration for custom sinks is deferred to
a future spec revision.

---

## 13. Arrays

Self's `array` is a primitive indexable object type baked directly into the
VM (a variable-sized object whose "slots" are numerically indexed storage,
not named data/var/method/parent slots) — not something built out of the
ordinary slot model. Self also has `byteVector`/`objVector` distinctions and
a much richer collection hierarchy (`orderedCollection`, `dictionary`,
`set`, …) on top, per `stdlib.md` § Collections.

**Ego stance — adopt the primitive-indexable shape, minimal Stage 1 slice.**

- One `ObjectKind::Array(Vec<ObjectId>)` in the tree-walker; no
  byte/object-vector distinction (ego has no byte-level types yet).
- Fixed size, set at creation (`array new: n`); no resizing primitive.
- 1-based indexing (`at:`/`at:Put:`), matching Self and `stdlib.md`.
- Only `new:`, `at:`, `at:Put:`, `size`, `printString` at Stage 1 — same
  precedent as strings getting only `,`/`printString` at substage 1.12
  (§8 of `self-notes.md`... see the Strings row in `lang-spec.md` §8).
  The full `stdlib.md` Array API (`do:`, `collect:`, `select:`, `detect:`,
  …) needs blocks dispatched from inside a primitive, which the current
  `PrimFn` signature (`Arena` + `RootSet` only, no `Interpreter`) can't do —
  same limitation that pushed block `value`/`whileTrue:`/`on:Do:` into
  `eval_send` special-casing instead of the primitive table. Deferred.
- Added as substage 1.16, ahead of mirrors (1.17), purely because
  `slotNames` needs an array to return — not because collections were
  otherwise due this early.

---

## Ego Adoption Summary

| Self feature | Ego stance | Notes |
|---|---|---|
| Prototype-based objects | Adopt | Core to ego |
| Five slot kinds | Adopt | Same semantics, minor syntax diff for arg slots |
| `self` / `resend` semantics | Adopt | Dot-syntax + directed resend documented, not yet implemented |
| Prototype/traits split | Adopt as idiom | Not enforced by language |
| Lookup algorithm (depth-first, ambiguity error) | Adopt | |
| Blocks, `value`/`value:With:` activation | Adopt | Not `value:value:` — see §11 |
| `^` non-local return, dead-block error | Adopt | |
| Block closure via activation-object chain | Adopt | Object literals lack this — see §2, §5 |
| Object literal two-phase construction (lobby-scoped initializers) | Adopt | Spec in `lang-spec.md` §1 |
| Lobby as root | Adopt | Not a universal ancestor of other objects — see §6 |
| Cascades (`;`) | Adopt | Spec in `lang-spec.md` §9 |
| Mirror API (`reflect:`, stratified) | Adopt, simplified | No sub-mirrors in Stage 1 |
| Arrays (primitive indexable object) | Adopt shape, minimal Stage 1 slice | Fixed-size only, no `do:`/`collect:`/… — see §13 |
| `on:Do:` exception handling | Adopt | `ExceptionSet` deferred |
| Binary-operator restriction, keyword capitalization/nesting | Adopt | See §11; implemented in parser |
| String escapes, line continuation | Adopt | Documented, not yet implemented |
| Numeric tower (int/bignum/float) | Adopt | |
| Characters as separate type | Diverge | ego characters are integers (codepoints) |
| Image-based persistence | Diverge | Not in Stage 1; image design TBD |
| GUI / Morphic | Diverge | Out of scope |
| JIT compilation | Diverge | Not until Stage 3+ |
