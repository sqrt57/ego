# ego Roadmap

## Pre-Implementation

Decisions and artifacts needed before writing the interpreter:

- **Formal grammar** — ✓ [lang-grammar.md](lang-grammar.md).
- **Command line** — ✓ [cli.md](cli.md); REPL, script, and inline-eval modes.
- **Implementation platform** — pending. Analysis: [implementation-platform.md](implementation-platform.md).

Once the platform is chosen, implementation design goes in `ego-<platform>-impl.md`
(e.g. `ego-rs-impl.md` for Rust), mirroring this project's docs conventions.

## Implementation

Slices are vertical — each adds one feature group end-to-end, touching lexer,
parser, evaluator, and CLI/REPL together. This surfaces integration bugs
early and keeps the interpreter runnable throughout, rather than leaving
"the evaluator" untestable until every other phase is done.

### Slice 1 — End-to-end skeleton

Target program:

```
(| x = 42. answer = ( x ) |) answer
```

Parses an object literal with one data slot and one unary method slot,
sends it a unary message, and prints `42`. Exercises the full pipeline:
lexer (identifiers, integers, `(`/`)`/`|`/`.`), parser (object literals,
unary sends), evaluator (slot lookup, method activation), REPL/CLI.

Spec: [§ Objects and Slots](lang-spec.md#1-objects-and-slots),
[§ Messages](lang-spec.md#2-messages).

### Slice 2 — Var slots and sequences

Multiple slots per object, `<-` var slots with generated `name:` setters,
multi-statement top-level programs separated by `.`.

Spec: [§ Objects and Slots](lang-spec.md#1-objects-and-slots).

### Slice 3 — Binary and keyword messages

Binary methods (`+`, `-`, …) and keyword methods (`at:put:`-style) on
user-defined objects; built-in numeric arithmetic and comparison.

Spec: [§ Messages](lang-spec.md#2-messages),
[§ Built-in Objects](lang-spec.md#8-built-in-objects).

### Slice 4 — Parent slots and resend

`*` parent slots, message lookup falling through to parents, `resend` for
continuing lookup past the current method.

Spec: [§ Objects and Slots](lang-spec.md#1-objects-and-slots),
[§ Messages](lang-spec.md#2-messages).

### Slice 5 — Blocks

Block literals, closures over enclosing scope, `value`/`value:` family.

Spec: [§ Blocks](lang-spec.md#3-blocks).

### Slice 6 — Booleans and control flow

`true`/`false` prototypes, `ifTrue:ifFalse:`, `and:`, `or:`, `not` — no
special `if` syntax anywhere in the pipeline.

Spec: [§ Control Flow via Messages](lang-spec.md#7-control-flow-via-messages).

### Slice 7 — Loops

`whileTrue:` on blocks.

Spec: [§ Control Flow via Messages](lang-spec.md#7-control-flow-via-messages).

### Slice 8 — Strings

String literals, `,` concatenation, `printString` on built-ins.

Spec: [§ Literals](lang-spec.md#4-literals),
[§ Built-in Objects](lang-spec.md#8-built-in-objects).

### Slice 9 — Script files and error reporting

Running `.ego` files (not just REPL input), `line:column:` error diagnostics.

Spec: [cli.md](cli.md).

### Testing philosophy

Lexer and parser: table-driven unit tests (source text in, expected tokens/AST
out). Evaluator: golden-style tests — source text in, printed result out —
since there's no type checker or separate IR to unit-test in isolation.
End-to-end: one test per slice, running the slice's target program through
the CLI and asserting on stdout. As slices accumulate, this suite doubles as
a regression guard. Concrete test framework and project layout are decided
in `ego-<platform>-impl.md` once the platform is chosen.

## Future Work

Features worth revisiting once the core is working:

| Feature | Notes |
|---|---|
| Non-local block return | Needs activation-record semantics — design work before implementation |
| Exception handling | No mechanism designed yet |
| Modules / namespaces | Only the lobby exists for now |
| Mirror-based reflection | A defining feature of real Self; substantial design effort on its own |
| Numeric tower | Bignums, int/float coercion rules |
| Bytecode compilation / performance | The initial interpreter is not expected to be fast |
| Concurrency | Out of scope until the sequential core is solid |
