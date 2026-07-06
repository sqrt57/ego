# Implementation Platform

**Status: undecided.** This records the comparison so the eventual choice
(and the reasoning behind it) survives past the conversation it came from.
Once decided, add a `## Decision` section here and start `ego-<platform>-impl.md`
for the implementation design.

## The core technical question

ego's object model is a mutable, cyclic graph — slots point to objects,
blocks close over enclosing scope, parent slots point back up. This needs
either a real tracing GC or a deliberate substitute. That single fact
dominates the comparison below more than any other factor.

## Options — GC provided

**Approach pros:** No memory strategy to design upfront; cyclic object graphs work without special effort; faster path to first working slice.

**Approach cons:** Less control over object layout and memory; fewer systems-language options in this tier.

| Platform | Pros | Cons | Opportunities | Risks |
|---|---|---|---|---|
| **C#** | Mature tooling (`dotnet build/test`); pattern matching + records suit AST work; directly mirrors the existing `pintc-cs` setup | Less "systems" feel | Reuse conventions and CLAUDE.md patterns already proven in `pintc-cs` — near-zero setup cost | Low — the safest, most-precedented choice given prior history |
| **OCaml** | ADTs + exhaustive pattern matching are the textbook-best fit for interpreters | Smaller ecosystem than mainstream languages; `dune`/`opam` tooling has a learning curve | Terse, correct-by-construction AST/eval code | Claude's OCaml fluency is lower than Rust/C#/TS/Python — more risk of tooling/syntax friction |
| **TypeScript (Node)** | Extremely high Claude fluency; near-zero-compile iteration | Structural/optional typing — weaker guardrails than a real type checker unless disciplined | Fastest path to a throwaway reference interpreter to validate the spec | Bugs surface at test time, not compile time |
| **Python** | Highest Claude fluency; fastest to a first working slice | Weak/no static typing; slow at runtime (irrelevant for a hobby VM) | Same reference-prototype role as TS, even faster to slice 1 | Easiest to write, easiest to silently get wrong |
| **Go** | Simple, high fluency, fast compiles | No ADTs/sum types/pattern matching — AST dispatch becomes type-switches | Simplicity if the language stays small | More silent-bug surface (failed type assertions, forgotten switch cases) |

## Options — GC not provided

**Approach pros:** Full control over object layout and memory; no GC runtime overhead; systems-language experience throughout.

**Approach cons:** Memory strategy must be committed to upfront — getting this wrong early means heavy refactoring; more design decisions before the first slice runs. Two standard mitigations:
- **Arena/index-based object model** — objects live in a flat array, slots hold integer indices rather than pointers; no cycles in the ownership graph.
- **Conservative GC library** — e.g. Boehm GC for C; drops in as a transparent allocator replacement.
- **Custom tracing GC** — implement a simple mark-and-sweep or copying collector over a managed heap; full control, but significant upfront effort before any interpreter work begins.

| Platform | Pros | Cons | Opportunities | Risks |
|---|---|---|---|---|
| **Rust** | Fast; huge Claude fluency; `cargo test`/`clippy` give a tight feedback loop; memory-safe via borrow checker; `ego-old` already started here | Borrow checker fights cyclic ownership — `Rc<RefCell<>>` leaks, necessitating the arena pattern | `ego-old` as prior art for the Rust object model | Without the arena pattern decided upfront, expect heavy iteration churn |
| **C** | The traditional interpreter language; ubiquitous tooling (`gcc`/`clang`); very high Claude fluency; no borrow-checker friction; raw performance | No memory safety — use-after-free and leaks are silent; tagged unions via enum+struct with no exhaustive-switch enforcement | Arena allocators are simple and idiomatic in C | Memory errors won't be caught by the compiler; no exhaustive dispatch means forgotten cases are runtime bugs |
| **Odin** | No borrow-checker friction; tagged unions suit AST dispatch; arena allocators are idiomatic and first-class; fast compiles | Smaller ecosystem; fewer learning resources than mainstream languages | Allocators as a first-class language concept make the arena pattern cleaner than in C or Zig | Smaller Claude training corpus than Rust; tooling less mature than Rust/C# |
| **Zig** | No borrow-checker friction | Pre-1.0, breaking changes across versions | Systems-language experience without Rust's friction | Smallest corpus of this list, most version churn — highest chance of confidently-wrong code |

## Claude Code authoring rating

| Platform | Rating | Why |
|---|---|---|
| C# | Excellent | GC removes the hardest design problem outright; top-tier tooling and fluency; direct precedent in `pintc-cs` |
| TypeScript | Very good | Same GC advantage, fastest iteration loop; weaker compile-time guardrails than C# |
| Rust (arena pattern) | Good | Best-in-class compiler feedback everywhere except the object graph; the arena pattern neutralizes that weak spot, but must be committed to upfront |
| Python | Good, as a prototype | Best fluency, fastest first slice; not recommended as the sole/final implementation |
| OCaml | Fair | Best technical fit for the problem domain, but Claude's fluency gap costs more turns than the fit gains back |
| Go | Fair | Mechanically solid, but no exhaustive pattern matching invites silent-bug classes |
| C (Boehm GC) | Fair | Proven interpreter heritage and high fluency; Boehm GC sidesteps the cycle problem, but memory errors are silent and dispatch is not exhaustive |
| Odin (arena pattern) | Fair | Tagged unions and first-class allocators are a better fit than Zig; smaller corpus than Rust means more risk of subtly wrong generated code |
| Rust (naive `Rc<RefCell>`) | Risky | The likely failure mode if the arena pattern isn't decided upfront |
| C (arena, no GC) | Risky | All the memory-safety risk of C plus a custom object model to maintain — high effort, no compiler help |
| Zig | Risky | Smallest corpus, most version churn — highest chance of confidently-wrong code |

## Recommendation (not yet acted on)

Two reasonable paths: **C#** as the low-risk choice (reuses a pattern already
trusted from `pintc-cs`), or **Rust with an arena-based object model** as the
choice that honors `ego-old`'s direction without inheriting its hardest
problem unmitigated. Decision deferred to whoever/whenever this gets picked
up.

## Decision

**Multi-stage split architecture.** Primary motivation is learning; each stage
produces a working deliverable that is retained as later stages are added.

| Stage | Language | What it builds |
|---|---|---|
| 1 | Rust | Tree-walking interpreter + REPL |
| 2 | Rust | Bytecode compiler + bytecode VM; defines the bytecode file format |
| 3 | Zig | Advanced VM consuming the same bytecode format; custom GC |
| 4 | Ego | Self-hosted compiler targeting the bytecode format |

**Rationale:**

- **Rust for stages 1–2** sidesteps the cyclic-object-graph problem via an
  arena-based object model from the start: objects live in a flat array,
  slots hold integer indices rather than pointers, and a simple mark-and-sweep
  collector runs over the arena. This applies to both the tree-walker and the
  bytecode VM.
- **Zig for stage 3** is the learning artifact for memory management and GC
  design. The pre-1.0 risk is acceptable for a personal project and mitigated
  by pinning a specific Zig version; Zig 1.0 is expected before stage 3 begins.
- **No FFI boundary between Rust and Zig.** The interface between stages is the
  bytecode file format — a stable, version-tagged binary format designed at
  stage 2. This makes the seam an I/O contract rather than an FFI boundary and
  keeps each stage independently testable.
- **All stages remain available** after later stages ship. The tree-walker is
  retained as a reference implementation and test oracle; the Rust bytecode VM
  is retained as a fallback and debugging tool. Running identical programs
  through multiple runtimes is the primary cross-stage correctness check.

Implementation design per stage:

- Stage 1: `rs-treewalk-impl.md`
- Stage 2: `rs-vm-impl.md`; bytecode format: `bytecode.md`
- Stage 3: `zig-vm-impl.md`
- Stage 4: `self-hosted-impl.md`
