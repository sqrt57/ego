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
