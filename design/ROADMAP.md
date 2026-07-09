# ego Roadmap

## Pre-Implementation

- **Formal grammar** — ✓ [lang-grammar.md](lang-grammar.md)
- **Command line** — ✓ [cli.md](cli.md); REPL, script, and inline-eval modes
- **Implementation platform** — ✓ [implementation-platform.md](implementation-platform.md);
  multi-stage split: Rust tree-walker → Rust bytecode VM → Zig VM → self-hosted compiler
- **Self documentation review** — `self-notes.md`
  - Study Self 4.x language reference and papers
  - Extract decisions relevant to ego: exception handling protocol, mirror API,
    cascade semantics, block activation, numeric tower behaviour
  - Document what ego adopts, adapts, or intentionally diverges from
  - Should precede the language spec completion work
- **Complete language spec** — additions to `lang-spec.md`
  - § Cascades — ✓ [lang-spec.md §9](lang-spec.md#9-cascades)
  - § Exception Handling — ✓ [lang-spec.md §10](lang-spec.md#10-exception-handling)
  - § Mirrors — ✓ [lang-spec.md §11](lang-spec.md#11-mirrors)
  - Each section must exist before its corresponding Stage 1 substage begins
- **Standard library spec** — ✓ [stdlib.md](stdlib.md)
  - Collections: array, orderedCollection, dictionary
  - I/O: console (stdin/stdout/stderr), file streams
  - String utilities beyond core spec
  - Numeric utilities (integer, float, math object)
  - Nil-testing convention (`isNil`/`notNil` on built-in objects)
- **Image workflow design** — deferred to Stage 2; not needed for Stage 1
  (Stage 1 has no image: every run parses source, evaluates, and discards the
  object graph). Design doc `image.md` to be written before Stage 2 begins.

---

## Stage 1 — Rust Tree-Walking Interpreter

**Deliverable:** A working Ego interpreter and REPL written in Rust, using a
tree-walking evaluator. Object graph uses an arena-based model (objects in a
flat array, slots hold integer indices) with a simple mark-and-sweep GC. This
model is shared with Stage 2, reducing rework. This interpreter is retained
indefinitely as a reference implementation and test oracle.

Design: ✓ `rs-treewalk-impl.md`

### Substage 1.1 — Lexer ✓

Implement the complete lexer for all tokens defined in
[lang-grammar.md](lang-grammar.md): identifiers, capitalised keywords,
integers, floats, strings, character literals, operators, punctuation,
annotations, comments. No partial coverage — the grammar is complete and
small enough to implement fully in one pass.

Tests: table-driven unit tests, source text in → token stream out.

Spec: [§ Literals](lang-spec.md#4-literals), [lang-grammar.md](lang-grammar.md).

### Substage 1.2 — Parser ✓

Implement the complete parser producing a full AST for all constructs in the
grammar: object literals, all three message types, cascades, sequences, blocks,
return expressions, annotations.

Tests: table-driven unit tests, source text in → AST out. Parser tests are
independent of the evaluator and remain valid across all later stages.

Spec: [lang-grammar.md](lang-grammar.md), [lang-spec.md](lang-spec.md).

### Substage 1.3 — Object model

Arena-based object representation: objects live in a flat array, slots hold
integer indices rather than pointers. Define the slot kinds (data, method,
parent, var) and the basic lookup algorithm. No evaluation yet — just the
data structures and slot access.

Tests: unit tests for object construction and slot lookup in isolation.

Spec: [§ Objects and Slots](lang-spec.md#1-objects-and-slots).

### Substage 1.4 — GC

Mark-and-sweep collector over the arena from substage 1.3. Define the root
set, implement marking (following slot indices), implement sweeping (compaction
or free-list). GC must be correct before any evaluator work begins.

Tests: unit tests that allocate objects, drop roots, trigger collection, and
verify reclamation.

### Substage 1.5 — Evaluator: literals and REPL

Evaluate integer and float literals, `self`, `nil`, `true`, `false`. Wire up
the REPL loop and basic CLI (`-e`, script file). `printString` on built-in
numeric types. This is the first substage where the interpreter is runnable.

Target:

```
42
```

Spec: [§ Literals](lang-spec.md#4-literals), [§ Built-in Objects](lang-spec.md#8-built-in-objects),
[cli.md](cli.md).

### Substage 1.6 — Evaluator: objects and unary sends

Object literals, constant data slots, unary method slots, `self` inside
methods, unary message dispatch. Multi-statement sequences separated by `.`.

Target:

```
(| x = 42. answer = ( x ) |) answer
```

Spec: [§ Objects and Slots](lang-spec.md#1-objects-and-slots),
[§ Messages](lang-spec.md#2-messages).

### Substage 1.7 — Evaluator: var slots

`<-` var slots, auto-generated `name:` setter methods, mutation via setter.

Spec: [§ Objects and Slots](lang-spec.md#1-objects-and-slots).

### Substage 1.8 — Evaluator: binary and keyword messages

Binary methods and keyword methods on user-defined objects; built-in numeric
arithmetic (`+ - * /`) and comparison (`< > <= >= = ~=`); int/float coercion
rules (what happens when integer and float operands are mixed).

Spec: [§ Messages](lang-spec.md#2-messages),
[§ Built-in Objects](lang-spec.md#8-built-in-objects).

### Substage 1.9 — Evaluator: parent slots and resend ✓

`*` parent slots, message lookup falling through to parents, `resend` for
continuing lookup past the current method.

Spec: [§ Objects and Slots](lang-spec.md#1-objects-and-slots),
[§ Messages](lang-spec.md#2-messages).

### Substage 1.10 — Evaluator: blocks ✓

Block literals, closure capture over enclosing scope, `value`/`value:` family,
`^` non-local return.

Spec: [§ Blocks](lang-spec.md#3-blocks).

### Substage 1.11 — Evaluator: booleans and control flow ✓

`true`/`false` prototypes, `ifTrue:False:`, `and:`, `or:`, `not`,
`whileTrue:` — no special syntax anywhere in the pipeline.

Spec: [§ Control Flow via Messages](lang-spec.md#7-control-flow-via-messages).

### Substage 1.12 — Evaluator: strings ✓

String literals, `,` concatenation, `printString` on all built-in types.

Spec: [§ Literals](lang-spec.md#4-literals),
[§ Built-in Objects](lang-spec.md#8-built-in-objects).

### Substage 1.13 — Script files and error reporting ✓

Running `.ego` files from the CLI, `line:column:` error diagnostics with
source location tracking through lexer, parser, and evaluator.

Spec: [cli.md](cli.md).

### Substage 1.14 — Evaluator: cascades ✓

`;` cascade syntax — sends a sequence of messages to the same receiver
without repeating it.

```
collection add: 1; add: 2; add: 3.
```

Spec: [lang-spec.md](lang-spec.md#9-cascades).

### Substage 1.15 — Evaluator: exception handling ✓

Message-based exception handling consistent with Self: `[...] on: E Do: [:e | ...]`
as a keyword message sent to a block. Exception types are prototype objects.
Handlers are blocks receiving the exception object, which can be resumed,
retried, or re-raised via messages. Built-in signals: message not understood,
dead non-local return, arithmetic errors.

Spec: [lang-spec.md §10](lang-spec.md#10-exception-handling).

### Substage 1.16 — Evaluator: arrays ✓

Fixed-size, indexed sequences — the minimal collection type, added here
because substage 1.17 (mirrors) needs `slotNames` to return one and no
substage before this point has any array-returning built-in. `array new: n`
creates an n-element array with every slot initialised to `nil`; `at:`/
`at:Put:` are 1-based and signal `error` when out of range; `size` and
`printString` round out the minimal slice. The richer collection API
(`do:`, `collect:`, `select:`, `OrderedCollection`, `Dictionary`, …) stays
in `stdlib.md`, deferred beyond Stage 1 — same precedent as strings getting
only `,`/`printString` at substage 1.12.

Spec: [§ Built-in Objects](lang-spec.md#8-built-in-objects).

### Substage 1.17 — Evaluator: mirror-based reflection ✓

Mirror objects for introspective access to an object's slots — read slot
names and values, add or remove slots, without building these operations
into the core message dispatch. Mirrors keep the base object model clean.
Needs substage 1.16's arrays: `slotNames` returns one.

Spec: [lang-spec.md §11](lang-spec.md#11-mirrors).

### Substage 1.18 — Evaluator: bignums

Arbitrary-precision integers. Small integers (fitting a machine word) are
handled from substage 1.5; this substage adds transparent promotion to
bignums when arithmetic overflows, so integer arithmetic is exact without
bounds. Use a library (e.g. `num-bigint` in Rust) rather than implementing
arithmetic from scratch.

Spec: [§ Built-in Objects](lang-spec.md#8-built-in-objects) — extend the
Numbers row.

### Testing philosophy

Lexer and parser tests are table-driven unit tests (source text in, expected
tokens/AST out) written at substages 1.1–1.2 and never need to change.
Evaluator tests are golden-style (source text in, printed result out), one
suite growing from substage 1.5 onward. This golden suite doubles as the
cross-stage regression guard — all later stages run it and must produce
identical output. Concrete test framework and project layout are decided in
`rs-treewalk-impl.md`.

---

## Stage 2 — Rust Bytecode Compiler and VM

**Deliverable:** A bytecode compiler (source → `.egoc` files) and a bytecode
VM, both written in Rust. The bytecode file format is the long-lived contract
between all subsequent stages — design it for the Zig VM's needs, not just
Rust's convenience. The Stage 1 tree-walker is retained alongside this.

Design: `rs-vm-impl.md`. Bytecode format: `bytecode.md`.

### Bytecode format design

The bytecode format is the long-lived contract between all subsequent stages.
Design it before writing the compiler.

**Register-based vs stack-based** — decide at Stage 2 based on experience from
Stage 1:

| | Stack-based | Register-based |
|---|---|---|
| Examples | CPython, clox, JVM | Lua 5.x, Dalvik, V8 Ignition |
| Instructions | More, smaller | Fewer, larger |
| Compiler | Simpler to write | Slightly harder |
| VM dispatch | More dispatches per operation | Fewer |
| Typical choice for | Pedagogical/small VMs | Performance-oriented VMs |

Stack-based is easier to compile to and easier to debug; register-based reduces
dispatch overhead in the Zig VM. Either is defensible — commit to one before
writing the compiler.

**Reference formats to study:**

- **Lua 5.x** — compact, elegant register-based design; small readable C source;
  the closest match in scope and style to ego.
- **clox** (Crafting Interpreters, Nystrom) — stack-based; the book walks through
  every design decision explicitly. Free online.
- **Self VM papers** (Chambers, Ungar, 1989+) — not a bytecode format (Self
  JIT-compiled), but covers maps, polymorphic inline caches, and prototype chain
  traversal — the object model closest to ego.
- **Squeak/Pharo (Smalltalk)** — relevant for block/closure representation and
  method lookup; Self evolved from Smalltalk-80.
- **CPython / YARV** — useful for closure upvalue design and dynamic-language
  frame layout.
- **WebAssembly binary format** — not a dynamic-language VM, but exemplary file
  format conventions: magic bytes, version field, section types, LEB128 encoding.

Key milestones (detailed slices go in the impl doc):

- **Bytecode format spec** — instruction set, object representation, file format
  with magic bytes and version tag. Decide register vs stack first.
- **Compiler** — source → bytecode, feature-parity with Stage 1 slices 1–9.
- **VM** — bytecode dispatch loop, arena-based object model (objects in a flat
  array, slots hold integer indices), mark-and-sweep GC.
- **REPL integration** — incremental compilation, consistent CLI with Stage 1.
- **Cross-stage test oracle** — all Stage 1 golden tests run against the
  bytecode VM and produce identical results.

---

## Stage 3 — Zig VM

**Deliverable:** A high-performance VM written in Zig that reads the same
`.egoc` bytecode format defined at Stage 2. This is the learning artifact
for memory management and GC implementation. Stages 1 and 2 are retained.

Pin a specific Zig version at the start; do not upgrade mid-stage.

Design: `zig-vm-impl.md`.

Key milestones:

- **Object model** — decide heap layout, pointer tagging, and handle scheme.
- **Bytecode loader** — read and validate `.egoc` files.
- **Dispatch loop** — execute all instruction types defined at Stage 2.
- **GC** — custom tracing collector (mark-and-sweep to start; copy/generational
  as a later refinement).
- **Parity** — all Stage 2 tests pass against the Zig VM.
- **Performance baseline** — benchmark against the Rust VM on representative
  programs.

---

## Stage 4 — Self-Hosted Ego Compiler

**Deliverable:** The Ego compiler written in Ego, targeting the `.egoc` bytecode
format. The Zig VM runs it. Stages 1–3 are retained; the self-hosted compiler
becomes the primary way to build Ego programs.

Design: `self-hosted-impl.md`.

Prerequisites: Ego must be expressive enough to write a compiler — string
manipulation, I/O, algebraic-dispatch via prototype chains. These capabilities
are developed during Stage 1–2 ecosystem work (see below) and must be complete
before Stage 4 begins.

Key milestones:

- **Bootstrap** — compile a minimal subset of Ego using the self-hosted
  compiler running on the Stage 2 Rust VM. Verify output matches the Rust
  compiler's output for the same source.
- **Full parity** — self-hosted compiler handles all language features.
- **Compiler compiles itself** — classic metacircular milestone.

---

## Parallel — Ecosystem

These tracks proceed alongside the stage work, starting from Stage 1:

- **Standard library** — I/O, collections, string utilities; required by Stage 4
  and useful throughout development.
- **Test suite growth** — language-level tests written in Ego, runnable across
  all runtimes as new stages complete.
- **REPL tooling** — history, multiline input, introspection.
- **GUI** — deferred until the sequential core is solid (Stage 2 or later).

---

## Future Work

Features deferred until the core is working:

| Feature | Notes |
|---|---|
| GC refinement | Generational or copying collector once the simple mark-and-sweep baseline exists (Stage 3) |
| Concurrency | Out of scope until the sequential core is solid |
| Zig VM optimizations | JIT, inline caching, hidden classes — post-parity work |
