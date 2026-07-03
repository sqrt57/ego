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

## Options

| Platform | Pros | Cons | Opportunities | Risks |
|---|---|---|---|---|
| **Rust** | Memory-safe without GC pauses; fast; huge Claude fluency; `cargo test`/`clippy` give a tight feedback loop; `ego-old` already started here | No tracing GC — cyclic object graphs fight the borrow checker (`Rc<RefCell<>>` leaks cycles) | A generational-arena/index-based object model (objects live in a `Vec`, slots hold indices, not references) sidesteps the borrow checker entirely — the standard pattern real toy-VM projects use | Without the arena pattern decided upfront, expect heavy iteration churn fighting the object graph specifically |
| **C#** | Real tracing GC handles cycles for free; mature tooling (`dotnet build/test`); pattern matching + records suit AST work; directly mirrors the existing `pintc-cs` setup | Less "systems" feel; GC pauses (irrelevant at this scale) | Reuse conventions and CLAUDE.md patterns already proven in `pintc-cs` — near-zero setup cost | Low — the safest, most-precedented choice given prior history |
| **OCaml** | GC built in; ADTs + exhaustive pattern matching are the textbook-best fit for interpreters | Smaller ecosystem than mainstream languages; `dune`/`opam` tooling has a learning curve | Terse, correct-by-construction AST/eval code | Claude's OCaml fluency is lower than Rust/C#/TS/Python — more risk of tooling/syntax friction |
| **TypeScript (Node)** | Extremely high Claude fluency; GC built in; near-zero-compile iteration | Structural/optional typing — weaker guardrails than a real type checker unless disciplined | Fastest path to a throwaway reference interpreter to validate the spec | Bugs surface at test time, not compile time |
| **Python** | Highest Claude fluency; fastest to a first working slice | Weak/no static typing; slow at runtime (irrelevant for a hobby VM) | Same reference-prototype role as TS, even faster to slice 1 | Easiest to write, easiest to silently get wrong |
| **Go** | GC built in; simple, high fluency, fast compiles | No ADTs/sum types/pattern matching — AST dispatch becomes type-switches | Simplicity if the language stays small | More silent-bug surface (failed type assertions, forgotten switch cases) |
| **Odin** | No borrow-checker friction; tagged unions suit AST dispatch; arena allocators are idiomatic and first-class; fast compiles | No GC — same cyclic-graph problem as Rust/Zig; manual memory management | Arena-based object model is a natural fit — allocators are a core language concept, not a bolt-on | Smaller Claude training corpus than Rust; tooling less mature than Rust/C# |
| **Zig** | No GC needed, no borrow-checker friction — raw pointers + arenas | Manual memory management, no safety net; pre-1.0, breaking changes across versions | Systems-language experience without Rust's friction | Smallest training corpus of this list, plus version churn — highest chance of confidently-wrong code |

## Claude Code authoring rating

| Platform | Rating | Why |
|---|---|---|
| C# | Excellent | GC removes the hardest design problem outright; top-tier tooling and fluency; direct precedent in `pintc-cs` |
| TypeScript | Very good | Same GC advantage, fastest iteration loop; weaker compile-time guardrails than C# |
| Rust (arena pattern) | Good | Best-in-class compiler feedback everywhere except the object graph; the arena pattern neutralizes that weak spot, but must be committed to upfront |
| Python | Good, as a prototype | Best fluency, fastest first slice; not recommended as the sole/final implementation |
| OCaml | Fair | Best technical fit for the problem domain, but Claude's fluency gap costs more turns than the fit gains back |
| Go | Fair | Mechanically solid, but no exhaustive pattern matching invites silent-bug classes |
| Odin (arena pattern) | Fair | Tagged unions and first-class allocators are a better fit than Zig; smaller corpus than Rust means more risk of subtly wrong generated code |
| Rust (naive `Rc<RefCell>`) | Risky | The likely failure mode if the arena pattern isn't decided upfront |
| Zig | Risky | Smallest corpus, most version churn — highest chance of confidently-wrong code |

## Recommendation (not yet acted on)

Two reasonable paths: **C#** as the low-risk choice (reuses a pattern already
trusted from `pintc-cs`), or **Rust with an arena-based object model** as the
choice that honors `ego-old`'s direction without inheriting its hardest
problem unmitigated. Decision deferred to whoever/whenever this gets picked
up.

## Decision

TBD.
