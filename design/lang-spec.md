# ego Language Specification

## Philosophy

> Everything is an object. Behavior comes from cloning, not classifying.

No classes — objects are built by cloning an existing object and adding or
overriding slots. No special control-flow syntax — `if`, `and`, and loops are
ordinary messages sent to ordinary objects (mostly blocks and booleans), not
keywords baked into the grammar. If it can be a message send, it is one.

---

## Profile Summary

| Decision | Choice |
|---|---|
| **Object model** | Prototype-based — no classes, cloning instead of instantiation |
| **Dispatch** | Message passing: unary, binary, keyword messages |
| **Memory model** | Garbage collected |
| **Type system** | None — dynamically and uniformly typed, every value is an object |
| **Syntax** | Smalltalk/Self family |
| **Control flow** | Ordinary messages to booleans and blocks — no `if`/`while` keywords |
| **Error handling** | Message-based exception handling — `on:Do:` on blocks (§10) |
| **Concurrency** | None built-in |
| **Reflection** | Mirror-based (§11) |
| **Targets** | Multi-stage: Rust tree-walker → Rust bytecode VM → Zig VM → self-hosted compiler, see [implementation-platform.md](implementation-platform.md) |

---

## 1. Objects and Slots

An object is a collection of named **slots**. There are five slot kinds:

| Kind | Syntax | Meaning |
|---|---|---|
| Data slot | `name = expr.` | Holds a value; read via the unary message `name` |
| Var slot | `name <- expr.` | Like a data slot, but also generates a `name:` setter message that mutates it |
| Arg slot | `:name.` | Declares a parameter; used in standalone method objects (see below) |
| Method slot | `name = ( statements ).` | Holds executable code; invoked via message `name` |
| Parent slot | `name* = expr.` | Marks the slot's value as a parent — messages not found locally are looked up there |

Method slots come in three shapes, matching the three message kinds (§2):

```
"unary method"
double = ( self + self ).

"binary method"
+ other = ( self addTo: other ).

"keyword method"
between: lo And: hi = ( (self >= lo) and: (self <= hi) ).
```

Object literal syntax:

```
(|
    x = 0.
    y <- 0.
    parent* = somePrototype.
    printString = ( 'Point(' , x printString , ', ' , y printString , ')' )
|)
```

Slots are separated by `.`; the trailing `.` before `|)` is optional. An
object literal may also have a **code section** after the slot list — a
sequence of statements that runs when the object is activated as a method:

```
(| :x | x * x)
```

This form — arg slots plus a code section — is a standalone method object:
its code only runs once activated by a message send (out of scope for the
Stage 1 tree-walker today — see `design/backlog.md`). The same result is
more commonly written as a named method slot on a parent object:
`square: x = (x * x)`.

A code section with **no** arg slots behaves differently: since there is no
message send to later activate it, it runs immediately, right after the
slot list is built, with `self` bound to the object just constructed (there
is no message-send receiver to inherit `self` from). The literal's overall
value is the code section's last statement, not the freshly-built object:

```
(| i <- 0 | i: i + 1. i: i + 1. i)  "=> 2, not the object holding `i`"
```

A code section is optional; a literal with only slots and no code section
still evaluates to the constructed object itself, as before.

An optional annotation `{} = 'text'.` may appear at the start of the slot
list to attach metadata (e.g. a category name for IDE tooling).

An object is created either as a literal (above) or by **cloning** an
existing object (`anObject copy`), which produces a shallow copy: each slot
is reproduced with its original name, kind, and value, but slot values
themselves are not recursively cloned. Cloning is the only way to get a
"new instance" — there is no `new` keyword and no class/instance distinction.

### Two-phase construction

Building an object literal happens in two phases that run at different
times with different visibility:

1. **Slot initializers** — the `= expr` / `name* = expr` right-hand sides —
   evaluate once, at the point the literal is constructed, in the context
   of the **lobby** (§6). An initializer cannot reference another slot of
   the object being built (no forward reference, no `letrec`), and has no
   access to whatever method's `self` or temporaries happen to be in scope
   around the literal — only names reachable from the lobby.
2. **Method-slot bodies** run later, at invocation time, with `self` bound
   to the receiver of the message that invoked them (§2). A method body
   has no lexical link to the code that constructed the literal.

The consequence: a nested object literal's method body cannot see the
enclosing method's bindings, and has no automatic path to the lobby's
globals either — only its own slots and whatever its parent chain resolves
to. To give a nested object's method access to an outer or lobby binding,
capture it into a data slot via the initializer (which *is* lobby-scoped)
and read that slot from the method body:

```
(|
    total = someGlobal computeTotal.            "initializer: lobby-scoped"
    report = ( 'Total: ' , total printString )  "method body: reads `self total`"
|)
```

Blocks (§3) are the one exception to this rule: a block literal carries an
implicit link to the activation of its lexically enclosing block or
method, giving it genuine lexical access to outer locals and `self` — this
is what makes a block a closure and an object literal not.

---

## 2. Messages

Three kinds of message send, in increasing binding strength (loosest first):

| Kind | Example | Selector |
|---|---|---|
| Keyword | `dict at: 1 Put: 2` | `at:Put:` (parts concatenate) |
| Binary | `3 + 4` | `+` |
| Unary | `4 factorial` | `factorial` |

A keyword message's first part must start with a lowercase letter (`at:`);
only a message already in progress may continue with an uppercase-initial
part (`Put:`, `IfAbsent:`). Consecutive parts follow this rule to decide
whether they extend the current message or start a new one:

- An uppercase-initial part **extends** the message in progress:
  `dict at: 1 IfAbsent: [nil]` sends one message, `at:IfAbsent:`, with two
  arguments.
- A lowercase-initial part after the first **closes** the message in
  progress and **starts a new one**, nested as that message's trailing
  argument. This lets keyword sends chain right-to-left without
  parentheses:

  ```
  5 min: 6 min: 7 Max: 8 Max: 9 min: 10 Max: 11
  "= 5 min: (6 min: 7 Max: 8 Max: (9 min: 10 Max: 11))"
  ```

  Reading left to right: `5 min:` opens a message whose argument is `6`;
  the next part, `min:`, is lowercase, so it closes that message (final
  argument `6`) and opens a new one on `6`; `min:`'s argument is `7`, and
  the following `Max:` (uppercase) extends that same message to
  `min:Max:` with arguments `7, 8`; the next part is `Max:` again
  (uppercase), extending it further to `min:Max:Max:` with arguments
  `7, 8, 9`... except `9` is immediately followed by another lowercase
  `min:`, so `9` becomes the receiver of yet another new message instead of
  a plain argument. This is why the example above is a chain nested three
  deep, not one flat six-argument send.

  An unparenthesized keyword send can therefore only appear as the
  *trailing* argument of another keyword message (where this rule can pick
  it up) — never as a non-trailing argument, which is always parsed as a
  plain binary expression.

Precedence, tightest first: **unary > binary > keyword**. Parentheses override
precedence.

Binary messages have no associativity, except between identical operators,
which associate left to right. Mixing different binary operators
unparenthesized is a parse error — following Self rather than Smalltalk-80
here, since it closes off the classic `3 + 4 * 2` footgun.

```
3 + 4 factorial        "= 3 + (4 factorial)"
dict at: 1 Put: 2 + 3  "= dict at: 1 Put: (2 + 3)"
3 + 4 + 7              "= (3 + 4) + 7 -- identical operators associate left to right"
3 + 4 * 7              "parse error -- different binary operators require parentheses"
```

Any of the three message kinds may be sent with the receiver omitted, in
which case the receiver is `self`: `min: 5` means `self min: 5`, and `+ 3`
means `self + 3`. A bare identifier such as `i` is the unary case — it reads
a local variable or block parameter if one is in scope, and otherwise is an
implicit unary send of that name to `self`, which is how var-slot setters
get invoked without writing `self` (`i: i + 1` inside a block, §7).

`self` refers to the original message receiver, and is only meaningful
inside a method slot's body.

`resend` reaches an "overridden" method — one otherwise hidden because the
current object (or one of its parents) already defines a slot with the same
name. It is written as special syntax, not an ordinary message send: the
reserved word `resend`, an immediately-following `.` (no whitespace around
either side), and the message name:

```
printString = ( resend.printString , ' (custom)' ).
resend.+ 5
resend.min: 17 Max: 23
```

This **undirected resend** continues the lookup from the parent chain of
the object that defined the currently executing method. When a method is
reachable through more than one parent (an ambiguity that would otherwise
be a `messageNotUnderstood`-style error), a **directed resend** picks a
specific parent slot by name instead of `resend`, constraining the search
to that one parent and its own ancestors:

```
intParent.min: 17 Max: 23   "resend, but only searching through the intParent slot"
```

`resend` (undirected or directed) is only meaningful inside a method slot's
body.

`^` returns a value early from the enclosing method or block:

```
abs = (
    self < 0 ifTrue: [^ 0 - self].
    self
).
```

Without `^`, the result of a method or block is the value of its last
expression.

---

## 3. Blocks

A block is a closure literal:

```
[| :a. :b | a + b]
[42]
```

Parameters and local variables are declared between `|` delimiters before
the body, separated by `.` like object slots. Parameters use the `:name`
form; local variables use the same `name = expr` (data) or `name <- expr`
(var) syntax as object slots:

```
[| :x. sum <- 0 | sum <- sum + x. sum]
```

Blocks are invoked by sending `value` (zero params), `value:` (one param),
`value:With:` (two params), `value:With:With:` (three), and so on — ordinary
keyword/unary messages, not special syntax. The repeated part is
capitalized (`With:`, not `with:`) so it continues the same message instead
of starting a new, nested one (§2). A block's result is the value of its
last expression, or the value given to `^` if an early return is used.

Blocks close over the enclosing scope by reference, including local
variables and `self` at the point the block literal is evaluated.

**Reassigning a local variable** — `sum <- sum + x` above, inside the block
*body*, is not a slot mutation (§7's "no separate assignment operator" is
about slots specifically, mutated only via their generated `name:` setter).
A block or method local declared with `<-` is reassigned by writing
`name <- expr` again, anywhere a statement or expression may appear, not
just in the declaring header — this is the *only* place ego uses `<-` as a
genuine assignment operator, and it only applies when `name` is a bare local
already reachable in the enclosing lexical scope, never a slot on some other
object. A block's local variables (and re-evaluated `= expr`/`<- expr`
initializers) are rebound fresh every time the block is activated — they do
not persist between separate `value` sends to the same block object. State
that should persist across activations belongs in a var *slot* on an
enclosing object instead, mutated from inside the block via its `name:`
setter (§7's `whileTrue:` example does exactly this with `i: i + 1`).

---

## 4. Literals

| Literal | Syntax | Notes |
|---|---|---|
| Integer | `42`, `-7`, `16rFF`, `8r17` | Leading `-` is part of the literal, not a unary minus send. Optional `NrDIGITS` base prefix: base in decimal, then `r`, then value in that base |
| Float | `3.14`, `-1.5e10`, `2.0E-3` | Digit required on both sides of `.`; optional exponent `e`/`E` with optional sign |
| String | `'hello'` | Single-quoted; `''` inside a string is one literal `'` character |
| Block | `[\| :x \| x * x]` | See §3 |
| Object | `(\| ... \|)` | See §1 |

`true`, `false`, and `nil` are **not** keywords — they are ordinary
identifiers bound to global objects in the lobby (§6). Nothing about them is
special at the syntax level.

---

## 5. Comments

```
"This is a comment. Doubled quotes ("") embed a literal quote."
```

Double-quoted, like Smalltalk. Strings use single quotes — the two never
collide.

---

## 6. The Lobby

Top-level program text is a sequence of expressions separated by `.`,
each evaluated with an implicit receiver called the **lobby** — the root
object that provides access to built-ins (`true`, `false`, `nil`, number
and string prototypes, exception prototypes, etc.), the `reflect:` method
for obtaining mirrors (§11), and top-level bindings. There is no other
form of global state.

A REPL evaluates each top-level statement against the lobby as it is read
and prints the resulting object's `printString`.

### Reaching the lobby from elsewhere

Bare identifiers are implicit sends to `self` (§2), so an object only sees
the lobby's globals if the lobby is reachable through its own parent chain
— there is no separate name-resolution path for "globals" distinct from
ordinary message lookup. Every built-in prototype's trait guarantees this:
cloning from (or setting your `parent*` to) any standard-library object
gives you a path back to the lobby, so `true`, `false`, `nil`, and other
globals are always reachable from a method body built on the standard
library. An object with no parent slot at all — most commonly a bespoke
object literal that declares none — has no such path and cannot see the
lobby's globals unless one is added explicitly (§1's two-phase
construction note covers the mechanics of doing that from a slot
initializer).

---

## 7. Control Flow via Messages

There is no `if`, `while`, or `for` syntax. Control flow is ordinary
message sends to booleans and blocks:

```
(x > 0)
    ifTrue: ['positive']
    False: ['non-positive']

[i < 10] whileTrue: [
    i: i + 1
]
```

(`i: i + 1` sends the `i:` setter generated by `i`'s var slot — see §1. A
*slot* has no separate assignment operator; mutation is always a keyword
message. A block or method *local* variable is different — see §3's note on
reassigning `<-` locals. The second part of the conditional send is
capitalized — `ifTrue:False:`,
not `ifTrue:ifFalse:` — so it continues the same message instead of
starting a new, nested one, per the keyword-message grouping rule in §2.)

`ifTrue:False:`, `ifTrue:`, `ifFalse:`, `and:`, `or:`, and `not` are
ordinary keyword/unary methods on the `true`/`false` prototypes, taking
blocks where lazy evaluation is required. `whileTrue:` is an ordinary
keyword method on blocks.

---

## 8. Built-in Objects

The minimum needed to bootstrap:

| Object | Provides |
|---|---|
| `true`, `false` | `ifTrue:False:`, `ifTrue:`, `ifFalse:`, `and:`, `or:`, `not` |
| `nil` | The absence of a value; `isNil` → `true`, `notNil` → `false` |
| Numbers | Arithmetic (`+ - * /`), comparison (`< > <= >= = ~=`), `printString` |
| Strings | Concatenation (`,`), `printString` |
| Blocks | `value`, `value:`, `value:With:`, …, `whileTrue:` |
| Exception prototypes | `error` (base type), `messageNotUnderstood`, `badBlockActivation`, `zeroDivide`, `primitiveError`; all respond to `signal` and `signal:` (§10) |
| Arrays | Fixed-size indexed sequence: `array new: n`, `at:`, `at:Put:`, `size`, `printString` |

All built-in objects respond to `copy` (shallow clone, as described in §1)
and `printString` (returns a string representation). All built-in objects
except `nil` respond to `isNil` → `false` and `notNil` → `true`.

Integer arithmetic promotes transparently to bignums on overflow, and demotes
back to a plain integer if a later operation brings the result back within
range — a value has no separate "bignum" identity visible to ego code, just
one integer type. Mixed int/float expressions return float.

`array new: n` returns a fresh array of `n` elements, each initialised to
`nil`. Indexing is 1-based; `at:`/`at:Put:` signal `error` when the index is
out of range. This is the minimal slice needed to bootstrap mirrors (§11);
the richer collection API (`do:`, `collect:`, `OrderedCollection`,
`Dictionary`, …) is specified in `stdlib.md` and deferred beyond Stage 1.

---

## 9. Cascades

The `;` operator sends a sequence of messages to the same receiver without
repeating it:

```
collection add: 1; add: 2; add: 3.
```

The receiver is the receiver of the *message immediately before the first
`;`* — not that message's result. In `collection add: 1; add: 2; add: 3`,
the shared receiver is `collection`; `add: 1` is itself the first cascaded
message, sent to `collection` exactly like `add: 2` and `add: 3` are.

Only the outermost message before the first `;` is peeled apart this way.
Any messages nested inside *its* receiver expression are ordinary sends,
evaluated once as part of computing the shared receiver — they are not
themselves cascaded:

```
a foo bar: 1; baz.   "bar: 1, then baz — both sent to (a foo)'s result.
                       foo is not cascaded; it's part of computing the
                       receiver, and runs exactly once."
```

Each subsequent `;`-separated message is sent to that same receiver. The
result of the whole cascade is the result of the *last* message in the
chain.

All three message kinds may appear in a cascade:

```
stream nextPutAll: 'hello'; nl; close.
```

A cascade is not a statement separator — it binds more tightly than `.`. The
`.` that terminates the statement ends the entire cascade:

```
a foo; bar.   "cascade: foo and bar sent to a; result discarded"
b baz.        "separate statement"
```

---

## 10. Exception Handling

Exception handling is entirely message-based. There is no `try`/`catch`
syntax — a block is the protected region, and `on:Do:` is an ordinary keyword
message sent to it. The second part is capitalized (`Do:`, not `do:`) so it
continues the `on:` message instead of starting a new, nested one — see the
keyword-message grouping rule in §2.

### Protecting a region

```
[risky code] on: ExceptionType Do: [:e | handler].
```

The receiver block is evaluated. If an exception whose type is `ExceptionType`
(or a subtype, via its parent chain) is signalled during that evaluation, the
handler block is invoked with the exception object as its argument. The result
of the `on:Do:` expression is the result of whichever block completes normally.

### Signalling an exception

```
anException signal.
anException signal: 'description text'.
```

`signal` is an ordinary method on exception prototype objects. It unwinds the
stack searching for a matching `on:Do:` handler.

### Handler operations

Inside the handler block, the exception object `e` understands:

| Message | Effect |
|---|---|
| `e return` | Exits the `on:Do:` expression, returning `nil` |
| `e return: val` | Exits the `on:Do:` expression, returning `val` |
| `e retry` | Re-executes the protected block from the beginning |
| `e resume` | Resumes execution immediately after the `signal` send, returning `nil` to the signaller |
| `e resume: val` | Resumes after `signal`, returning `val` to the signaller |
| `e outer` | Passes the exception to the next enclosing handler for the same type |
| `e signal` | Re-raises the exception from the handler's location |
| `e messageText` | Returns the exception's description string |

If the handler block exits normally (no explicit operation), it is equivalent
to `e return:` with the block's value.

### Exception type hierarchy

Exception types are ordinary prototype objects. Subtyping is expressed via
parent slots — an exception is an instance of any type reachable through its
parent chain. `on:Do:` catches the named type and all of its subtypes.

Built-in exception types:

| Type | Signalled when |
|---|---|
| `error` | Base type; all built-in exceptions inherit from it |
| `messageNotUnderstood` | No method found for a message send |
| `badBlockActivation` | A non-local `^` return targets an already-returned method activation |
| `zeroDivide` | Division or modulo by zero |
| `primitiveError` | A built-in operation fails for any other reason |

User-defined exception types are created by cloning `error` (or any subtype)
and adding a parent slot pointing to the desired supertype.

---

## 11. Mirrors

Reflection is **separated from the base object model**. Ordinary objects have
no introspective methods — there is no `respondsTo:`, `perform:`, or `class`
on base objects. Reflection is accessed exclusively through a mirror object.

### Obtaining a mirror

```
| m |
m: reflect: anObject.
```

`reflect:` is a method on the lobby. It returns a mirror wrapping `anObject`.

### Mirror API

| Message | Returns |
|---|---|
| `m slotNames` | An array of slot-name strings for all slots in the reflectee |
| `m at: name` | The value of the slot named `name`; signals `error` if absent |
| `m at: name Put: val` | Assigns the slot named `name` to `val`; signals `error` if absent |
| `m addSlot: name Value: val` | Adds a new data slot named `name` with value `val` |
| `m removeSlot: name` | Removes the slot named `name`; signals `error` if absent |

`name` is always a string. Slot names include all slot kinds — data, var,
method, and parent slots are all visible.

### Design principles

1. **Encapsulation** — reflection requires possession of the mirror object.
   Code that does not receive a mirror cannot introspect the reflectee.

2. **Stratification** — the mirror's slot namespace is entirely separate from
   the reflectee's. A reflectee slot named `slotNames` does not collide with
   the mirror method `slotNames`.

3. **No reflective methods on base objects** — `respondsTo:`, `perform:`, and
   equivalents are not defined on any base object. Adding them to a user object
   is not prohibited, but the standard library does not provide them.
