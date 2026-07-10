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

## Object-literal body statements are parsed but never evaluated

`(| slots | body )` is real grammar — `parse_object_lit` (`parser.rs`) parses
trailing statements after the slot section's closing `|` into
`ObjectLit.body`, and `parser_tests.rs` has passing parser-level tests for
it (`object_with_body`, `object_no_slots_with_body`). But `eval_object_lit`/
`eval_object_slots` (`eval.rs`) only ever construct the new object's slots
from `obj.slots` — `obj.body` is read nowhere on the eval side, so any
statements written there are silently discarded; the object literal's value
is always the bare new object, never a body statement's result. Found while
writing substage 1.11 golden tests: `(| i <- 0 | someExpr. i)` looks like it
should run `someExpr` then answer `i`, but actually just answers a fresh
object with an `i` slot, full stop. Not fixed — no golden test currently
depends on evaluating an object literal's body (existing tests always put
"doit" logic in a method slot and send it after the closing paren, e.g.
`(| run = ( ... ) |) run`), and it's unclear from `lang-spec.md` alone what
the body's role is even meant to be (Self doesn't have this construct in
quite this shape — self-notes.md doesn't cover it). Needs a spec decision
before fixing: what should the literal's value be when a body is present,
and does the body run with `self` bound to the new object or the enclosing
one?

## ~~Bare `true`/`false`/`nil` (and other lobby bindings) are unreachable from inside most method bodies~~ — resolved by design, not a bug

**Resolved.** This entry (written during substage 1.11) asked whether
`Ident` should fall back to the lobby on a failed self-send, or whether
every object literal should get an implicit parent slot reaching the
lobby. Both questions were already answered — by commit `6f8c35b`
("Wire built-in traits back to the lobby"), which landed *before* this
entry's "not fixed" framing was last true, and is written up in
`self-notes.md` §6 under "Ego stance — adopt, with the reachability
question resolved": ego deliberately does **not** make the lobby a
universal ancestor. Built-in traits (`int_trait`, `bool_trait`,
`block_trait`, `array_trait`, `mirror_trait`, etc.) each get an explicit
`parent* → lobby` slot, guaranteeing reachability for anything cloned from
a stdlib prototype. A genuinely bespoke object literal with no parent
clause of its own gets none of this for free — matching Self's real
opt-out convention exactly, not a gap.

Verified empirically (2026-07-10, no code changes needed):
- Default opt-out still holds: `(| ok = ( true ) |) ok` →
  `message not understood: true`, as expected for a bespoke object with no
  path to the lobby.
- **Opt-in #1 — capture into a data slot.** Slot initializers always
  evaluate in lobby context (per the two-phase construction rule, `eval_object_slots`,
  fixed for *all* nesting depths by `b5a0fc7`, a substage-1.15 follow-up
  that postdates this entry's "bottoms out at the top level" caveat —
  that caveat no longer applies): `(| capturedTrue = true. ok = ( capturedTrue ) |) ok`
  → `true`. This is the sanctioned workaround already documented in
  `self-notes.md` lines 87–89.
- **Opt-in #2 — explicit parent clause.** `(| parent* = true. ok = ( false ) |) ok`
  → `false`: pointing a bespoke object's `parent*` at anything already
  lobby-reachable (here, the `true` singleton, itself parented to
  `bool_trait → lobby`) grants the whole chain.

No further action needed here. `Ident`'s self-send-only fallback in
`eval.rs` is correct as written.

## ~~`_PrintLine:` primitive exists but has no ego-level entry point~~ — fixed

**Resolved (2026-07-10).** `stdout` is now bound directly in `boot/boot.ego`
via `(reflect: self) addSlot: 'stdout' Value: (| ... |)` on the lobby —
mirror-based reflection (substage 1.17) made it possible to attach a new
prototype to the lobby entirely from `boot.ego`, no `bootstrap.rs` wiring
needed. Provides `print:`/`println:`/`show:`/`nl` per `stdlib.md`'s Console
table (`printString` too, so it doesn't hit the `-e`/REPL auto-print gap
every other trait already needed one for). `print:`/`println:` compute
`obj printString` themselves before handing a plain string to the
primitive, so they're written as real method bodies in `boot.ego` rather
than via `bootstrap.rs`'s raw-argument-forwarding `make_binary_prim_method`
helper. Added a new `_Print:` primitive (`primitives.rs`) alongside the
existing `_PrintLine:` — writes without a trailing newline, and explicitly
flushes stdout, since `main.rs` calls `std::process::exit` on several error
paths, which skips the flush-on-drop a normal return would get and could
otherwise strand a no-newline `print!` in the buffer.

`stdin`/`stderr` are still unimplemented — this only closes the write side.

Verified with a real subprocess-capturing CLI test (per this entry's own
suggestion), not just a golden test: `script_can_produce_explicit_output_via_stdout`
in `tests/cli_tests.rs` runs a script-mode `.ego` file through `stdout
show:`/`print:`/`nl`/`println:` and asserts on the actual captured process
stdout — the concrete thing this entry said golden tests couldn't do.

## ~~`parent*` on built-in protos unreachable by directed resend~~ — fixed

**Resolved.** The previous entry here claimed substage 1.17 (mirrors) made a
built-in multi-parent ambiguity gap "live, not just theoretical," reasoning
that `m addSlot:Value:`/`m at:Put:` could attach or overwrite a second parent
slot on `integer_proto` et al. That premise doesn't hold up against the
actual implementation: `_MirrorAddSlot:Value:` (`primitives.rs`) always
creates a `SlotKind::Data` slot, never `SlotKind::Parent`, and
`_MirrorAt:Put:` only overwrites an existing slot's `.value`, never its
`.kind` — so mirrors cannot currently create a second *real* parent slot on
anything. The ambiguity scenario remains theoretical.

There was a real, separate bug though: built-in protos' parent slots were
hardcoded in Rust as the literal string `"parent*"` (asterisk baked into the
stored name), which could never match a directed-resend target — the parser
strips the trailing `*` from `ParentSlotDecl` before storing the slot name,
so a user-written `parent* = X` was already stored as bare `"parent"`; the
Rust-side built-in construction just hadn't matched that convention. Fixed
by renaming every hardcoded parent-slot name from `"parent*"` to `"parent"`
across `bootstrap.rs`/`gc.rs`/`primitives.rs`/`eval.rs`. Verified via
`(reflect: 3) slotNames printString` → `"(parent)"` (was unreachable
`"(parent*)"` before). Full test suite green, no regressions. See
`self-notes.md` §11 for the updated writeup.

Still worth a future revisit if `addSlot:Value:` (or a new mirror primitive)
is ever extended to create genuine `SlotKind::Parent` slots — at that point
the naming-collision question (two parent slots on one object sharing a
name) becomes live and would need its own decision.
