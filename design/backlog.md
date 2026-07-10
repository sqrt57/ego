# ego Design Backlog

Open questions and known small inconsistencies, deliberately deferred rather
than blocking the substage that surfaced them. Resolve or fold into the
relevant design doc when the roadmap reaches the area again.

## Ambiguity-report format for message-not-understood

**Partially resolved (2026-07-10):** a real, live classification bug was
found and fixed here, separate from the "report format" question below.
`invoke_lookup` (`eval.rs`) was routing `lookup_in_parents`'s ambiguity
error through `ErrorKind::PrimitiveError`, not `ErrorKind::MessageNotUnderstood`
— so `on: primitiveError Do:` caught an ambiguous lookup but
`on: messageNotUnderstood Do:` did not, directly contradicting
`self-notes.md` §4's explicit rule ("signal a **message-not-understood**
with an ambiguity report"). Verified with a real `on:Do:` probe before
fixing (caught under `primitiveError`, uncaught/fatal under
`messageNotUnderstood`) and after (now caught under `messageNotUnderstood`,
as it should be). No test had ever asserted the exception *kind*, only that
the message text contained "ambiguous" (`ambiguous_parent_lookup_is_fatal`
in `eval_golden.rs`) — added a new golden test,
`1.9-parent-resend/ambiguous_lookup_is_catchable_as_message_not_understood.ego`,
that actually catches it via `on: messageNotUnderstood Do:` to close that
coverage gap. One-line fix in `eval.rs`, no other code touched.

**Still open:** the report itself remains plain prose naming just the
selector, with no enumeration of the competing parent objects/paths.
`self-notes.md` §4 doesn't specify the report's shape beyond "an ambiguity
report." Revisit if this needs more structure — no built-in exception
currently carries any data beyond `messageText` (not `zeroDivide`'s
divisor, not `messageNotUnderstood`'s receiver/selector), so adding
structured parent-path data here would be the first of its kind and wants
its own design decision, not a one-off.

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

## Standalone method objects (arg-slot literals) are never activated

`(| :x | x * x)` — an object literal with one or more `Arg` slots — is a
*standalone method object* per `lang-spec.md` §1: real Self activates it
(runs its code, with args bound from the message that reached it) whenever
it's found by a message lookup, the same way an ordinary `Method` slot's
body runs on lookup. ego's tree-walker doesn't implement this: `SlotKind::Arg`
lookup (`eval.rs`) returns the arg slot's placeholder `nil_id` value like an
ordinary data read, never activates anything, and arg-slot literals'
`ObjectLit.body` is left untouched by `eval_object_lit` even after the
no-arg-slot case was wired up (see the resolved entry below) — confirmed via
`(reflect: (| :x | x * x)) slotNames` still answering `(x)`, not running the
body. Noted as out of scope as far back as substage 1.9 ("Arg intentionally
left as `None` … standalone-method-object feature"). Fixing this needs: (1)
deciding how such an object gets *placed* somewhere activatable — as a data
slot's value, per `rs-treewalk-impl.md`'s "`(| … |)` as a data slot value"
note — and (2) teeing method lookup to check whether a found `Data`/`Var`
slot's value is itself a method object and, if so, activate it with args
from the message rather than returning it directly (mirrors how Self's
uniform slot model works — method slots and data-slot-holding-a-method-object
are the same underlying mechanism). Not attempted here; revisit as its own
substage if a real program needs first-class reusable method values.

## ~~Object-literal body statements are parsed but never evaluated~~ — fixed

**Resolved (2026-07-10).** The Self Handbook 2024.1 (§3.1.4, §3.1.6) settled
the spec question this entry raised: an object with a code section is a
*method object*, and "evaluating" one runs its code and answers the code's
result instead of the bare object. Adopted for the no-arg-slot case (the
only case with no existing "activation via message send" mechanism to
conflict with): `(| slots | body )` now runs `body` immediately after
`eval_object_slots` finishes, as a zero-arg method activation with `self`
bound to the newly-built object, and the literal's value becomes the body's
result — `(| i <- 0 | i: i + 1. i: i + 1. i)` now answers `2`, not a fresh
object. Literals *with* arg slots (real standalone method objects,
`lang-spec.md` §1) are untouched — their body still doesn't run, since
activating them needs args from a message send, which the tree-walker
doesn't implement (`SlotKind::Arg` lookup is still a no-op, unchanged, see
below). Implemented in `eval_object_lit` (`eval.rs`); docs updated in
`lang-spec.md` §1 and `rs-treewalk-impl.md` ("Object-literal code
sections"). 3 new golden tests in `tests/eval_golden/1.6-objects/`
(body-runs-after-slots, self-is-the-new-object, arg-slot literal's body
still not auto-run). Full suite green, no existing test depended on the old
discard-the-body behavior.

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
