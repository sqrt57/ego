# ego Design Backlog

Open questions and known small inconsistencies, deliberately deferred rather
than blocking the substage that surfaced them. Resolve or fold into the
relevant design doc when the roadmap reaches the area again.

## Ambiguity-report format for message-not-understood

`self-notes.md` §4 (lookup algorithm) says an ambiguous lookup (a selector
reachable through more than one parent) should "signal a
message-not-understood with an ambiguity report," without specifying what
the report should contain. Substage 1.9's implementation
(`lookup_in_parents` in `eval.rs`) signals ambiguity with a plain one-line
message naming the selector, with no enumeration of the competing parent
paths. Revisit once exception handling (substage 1.15) defines a real
error-object shape — an ambiguity report likely wants to carry the list of
parents/paths that matched, not just prose.

## `Data`-kind block/method locals aren't protected from reassignment

Object `Data` slots are immutable by construction — no setter method is ever
generated for them, so a message send can't mutate one. Substage 1.10's
`ExprKind::Assign` (`identifier <- expr` as a body statement, reassigning a
block/method local — see `rs-treewalk-impl.md`'s "Local variable assignment"
section) doesn't carry the same distinction: it writes into `activation.env`
unconditionally regardless of whether the name was declared via `name = expr`
(`LocalKind::Data`) or `name <- expr` (`LocalKind::Var`) in the block/method
header. A `Data` local can currently be reassigned exactly like a `Var` one.
Enforcing this would mean tracking `LocalKind` per env entry (today `Env` is
a flat `HashMap<String, ObjectId>` with no per-key metadata), touching every
insertion site (block/method params, block locals, `Assign`). Revisit if a
real program's correctness starts depending on `Data`-local immutability, or
alongside a broader `Env` rework.

## Recursive/reentrant block self-invocation can clobber its own params

Substage 1.10's block activation (`eval_block_call` in `eval.rs`) binds
params and evaluates locals' initializers directly into the block's
`captures` — the same shared `Env` the enclosing method activation uses, not
a fresh child frame per invocation (matches `rs-treewalk-impl.md`'s
documented design: "Bindings for local variables and block parameters are
stored in a shared, reference-counted frame"). This is fine for the common
case — the same block invoked repeatedly in sequence (a loop body, a
counter) — since each call's writes are meant to be visible afterward. It
breaks for a block that recursively invokes *itself* through its own
captured var slot: the inner call overwrites the outer call's param
bindings in the shared table, so if the outer frame re-reads a param *after*
the recursive call returns (rather than only before, as in
`k * (fib value: k - 1)`, where the left operand is evaluated to a plain
`ObjectId` before recursing), it sees the inner call's value instead of its
own. Not fixed, not hit by any golden test. Fixing it would mean giving each
block *activation* (not just each block-literal evaluation) a fresh child
frame that falls back to `captures` for names it doesn't own — a real `Env`
redesign (today a flat `HashMap`, no parent-chain concept), out of scope
unless a real recursive-block use case demands it.
