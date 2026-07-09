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

## Bare `true`/`false`/`nil` (and other lobby bindings) are unreachable from inside most method bodies

`ExprKind::Ident` (`eval.rs`) resolves a bare identifier not found in the
current activation's env via `eval_send(activation.self_obj, name, ...)` —
an implicit unary send to `self`. The comment there already flags the
consequence: "At top-level `self_obj` is the lobby, so lobby slots are
found here too" — meaning this only works by coincidence at the top level.
Inside an ordinary method or block, `self` is the receiver, which has no
parent slot pointing at the lobby (`eval_object_slots` never gives new
objects an implicit parent), so a bare `true`/`false`/`nil`/`stdout`/etc.
reference fails with "message not understood" unless the enclosing object
happens to have its own same-named slot. `self-notes.md` §6 states the
intended stance plainly — "all names resolve through [the lobby] or the
receiver" — but only the receiver half is implemented.

Found while writing substage 1.11's `whileTrue:` golden tests: a condition
block `[false]` written *inside* a method failed with "message not
understood: false"; worked around by writing `[1 > 3]` instead (obtains the
same boolean via a comparison primitive, sidestepping identifier lookup
entirely) — matches the pattern every earlier substage's golden tests
already followed (booleans always came from comparisons, never a bare
`true`/`false` token, which is presumably why this was never hit before
1.11). Slot initializers evaluated in an *enclosing* activation (per the
1.9 finding on object-literal slot values) are fine whenever that enclosing
activation eventually bottoms out at the top level; it's specifically
method/block bodies where `self` is a non-lobby-descended object that break.

Not fixed — needs a design decision, not just a bug fix: should `Ident`
fall back to sending to the lobby when the self-send fails (cheap, but
silently changes what "message not understood" means for a genuinely
unbound name)? Should every object literal get an implicit parent slot
reaching the lobby (closer to Self's real universal-ancestor chain, but a
bigger change touching `eval_object_slots` and possibly GC root-marking)?
This will only get more pressing as later substages add `nil`-testing,
exception prototypes, and mirrors, all of which are lobby bindings that
method bodies will need to reach routinely.

## `_PrintLine:` primitive exists but has no ego-level entry point

`primitives.rs` registers `_PrintLine:` (writes a string to stdout plus a
newline) and `rs-treewalk-impl.md`'s primitive table documents it, but
nothing in `boot.ego` calls it — there is no `stdout`/`printLine:`/`show:`
wired up yet (`stdlib.md`'s `Console` section, § "The lobby binds `stdin`,
`stdout`, and `stderr`," is still just a spec, not implemented). Concretely,
an `.ego` *script* currently has no way to produce explicit output at all;
the only visible output is the REPL's/`-e`'s automatic `printString` of a
fragment's last expression, which script mode deliberately suppresses (see
`cli.md`'s script-mode rule). Found while writing substage 1.13's CLI
integration tests (`tests/cli_tests.rs`): those tests can only assert on
exit codes, the auto-print/no-print rule, and `file:line:col:` diagnostics —
they can't exercise a script actually printing something, since there's
currently nothing in the language that lets it. Revisit once the
collections/IO substage wires up `stdout`/`stderr`/`stdin` per `stdlib.md`;
at that point add a golden or CLI test that drives real script output
through `_PrintLine:`.

## `parent*` on built-in protos is unreachable by directed resend — now that mirrors can add a second one

`self-notes.md` §11 (lines 521-530) notes that every built-in numeric/string
prototype's parent slot is named literally `"parent*"` (asterisk included),
which is not a producible `identifier` and so can't be targeted by directed
resend or written as an ordinary send — deliberately fine as long as the
built-in chain stays strictly linear (one parent per link), since undirected
`resend` already reaches the whole chain unambiguously. That note explicitly
flagged substage 1.17 (mirrors) as the point where this stops being purely
hypothetical: `m addSlot:Value:`/`m at:Put:` (mirror API, `lang-spec.md` §11)
can now attach or overwrite a second parent slot on `integer_proto` et al.
from ordinary ego code, at which point a selector reachable through both
parents becomes genuinely ambiguous with no way to directed-resend to either
one by name. Substage 1.17 shipped (`0198668`) without addressing this —
mirrors don't special-case `parent*`-named slots, so the gap is now live,
not just theoretical. Revisit by giving built-in protos' parent slots a
producible name (dropping the `*` convention, or reserving a distinct
nameable alias) before relying on multi-parent built-ins anywhere.
