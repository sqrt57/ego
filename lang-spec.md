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
| **Error handling** | Message-based exception handling — `on:do:` on blocks; spec section pending |
| **Concurrency** | None built-in |
| **Reflection** | Mirror-based — spec section pending |
| **Targets** | Reference interpreter only — implementation platform not yet decided, see [implementation-platform.md](implementation-platform.md) |

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
between: lo and: hi = ( (self >= lo) and: (self <= hi) ).
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

This form — arg slots plus a code section — is a standalone method object.
The same result is more commonly written as a named method slot on a parent
object: `square: x = (x * x)`.

An optional annotation `{} = 'text'.` may appear at the start of the slot
list to attach metadata (e.g. a category name for IDE tooling).

An object is created either as a literal (above) or by **cloning** an
existing object (`anObject copy`), which duplicates its slots. Cloning is
the only way to get a "new instance" — there is no `new` keyword and no
class/instance distinction.

---

## 2. Messages

Three kinds of message send, in increasing binding strength (loosest first):

| Kind | Example | Selector |
|---|---|---|
| Keyword | `dict at: 1 put: 2` | `at:put:` (parts concatenate) |
| Binary | `3 + 4` | `+` |
| Unary | `4 factorial` | `factorial` |

Keyword parts may start with a lowercase letter (`at:`, `put:`) or an
uppercase letter (`IfTrue:`, `IfFalse:`). Both kinds participate in the same
selector accumulation: `dict at: 1 IfAbsent: [nil]` sends the single message
`at:IfAbsent:` with two arguments.

Precedence, tightest first: **unary > binary > keyword**. Same-precedence
messages associate left to right. Parentheses override precedence.

```
3 + 4 factorial        "= 3 + (4 factorial)"
dict at: 1 put: 2 + 3  "= dict at: 1 put: (2 + 3)"
```

A keyword message's parts always extend to the right — you cannot write an
unparenthesized keyword send as the argument of another keyword send.

`self` refers to the original message receiver. `resend` is a pseudo-object
used exactly like `self`, but continues the method lookup search from the
current method's parent, instead of restarting it — the mechanism for calling
an "overridden" method:

```
printString = ( resend printString , ' (custom)' ).
```

Both `self` and `resend` are only meaningful inside a method slot's body.

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
[| :a :b | a + b]
[42]
```

Parameters and local variables are declared between `|` delimiters before
the body. Parameters use the `:name` form; local variables use the same
`name = expr` (data) or `name <- expr` (var) syntax as object slots:

```
[| :x. sum <- 0 | sum <- sum + x. sum]
```

Blocks are invoked by sending `value` (zero params), `value:` (one param),
`value:value:` (two params), and so on — ordinary keyword/unary messages,
not special syntax. A block's result is the value of its last expression,
or the value given to `^` if an early return is used.

Blocks close over the enclosing scope by reference, including local
variables and `self` at the point the block literal is evaluated.

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
and string prototypes, etc.) and top-level bindings. There is no other
form of global state.

A REPL evaluates each top-level statement against the lobby as it is read
and prints the resulting object's `printString`.

---

## 7. Control Flow via Messages

There is no `if`, `while`, or `for` syntax. Control flow is ordinary
message sends to booleans and blocks:

```
(x > 0)
    ifTrue: ['positive']
    ifFalse: ['non-positive']

[i < 10] whileTrue: [
    i: i + 1
]
```

(`i: i + 1` sends the `i:` setter generated by `i`'s var slot — see §1. ego
has no separate assignment operator; mutation is always a keyword message.)

`ifTrue:ifFalse:`, `ifTrue:`, `ifFalse:`, `and:`, `or:`, and `not` are
ordinary keyword/unary methods on the `true`/`false` prototypes, taking
blocks where lazy evaluation is required. `whileTrue:` is an ordinary
keyword method on blocks.

---

## 8. Built-in Objects

The minimum needed to bootstrap:

| Object | Provides |
|---|---|
| `true`, `false` | `ifTrue:ifFalse:`, `ifTrue:`, `ifFalse:`, `and:`, `or:`, `not` |
| `nil` | The absence of a value |
| Numbers | Arithmetic (`+ - * /`), comparison (`< > <= >= = ~=`) |
| Strings | Concatenation (`,`), `printString` |
| Blocks | `value`, `value:`, `value:value:`, …, `whileTrue:` |

Integer arithmetic promotes transparently to bignums on overflow. Mixed
int/float expressions return float.
