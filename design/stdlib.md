# ego Standard Library

## Scope

This document specifies the standard library objects that supplement the core
language spec. It is sufficient to guide Stage 1 parallel ecosystem work and
is not an exhaustive specification of all future library content.

**In scope:**
- Collections: `array`, `orderedCollection`, `dictionary`
- I/O: console streams, file streams
- String utilities beyond the core spec
- Numeric utilities beyond the core spec

**Deferred:**
- Networking, processes, FFI
- GUI (Stage 2 or later; see Roadmap)
- Module/package system (not planned; the lobby is the single namespace)

---

## Conventions

Object descriptions follow the **prototype/traits idiom** (see `self-notes.md` §3):
a prototype object holds default data slots and a `parent*` slot pointing to a
traits object, which holds all method slots. This is a documentation convention,
not enforced by the language.

In message signatures:
- `n`, `m` — integers
- `s` — string
- `obj` — any object
- `aBlock` — a block
- `key`, `val` — arbitrary objects (used in collection context)

**Indices are 1-based** throughout, consistent with Smalltalk/Self.

Error conditions signal exceptions from the `error` hierarchy (§10 of
`lang-spec.md`) unless otherwise noted.

---

## Nil Testing

Several ecosystem methods return `nil` as a sentinel (e.g. `readLine` at EOF,
`detect:IfNone:` default). To make nil-testing practical without reflective
methods on every object, the following extension to `lang-spec.md` §8 applies:

- `nil` responds to `isNil` → `true` and `notNil` → `false`.
- All other built-in objects respond to `isNil` → `false` and `notNil` → `true`.
- User objects do not inherit these methods automatically; define them where needed.

---

## Collections

### Array

`array` — a fixed-size, indexed sequence. Size is fixed at creation time.

**Creating:**

```
array new: n          "n-element array; all slots initialised to nil"
```

`array` and `array traits` live in the lobby. Cloning is the mechanism for
creating new array instances internally; user code uses `new:`.

**Messages:**

| Message | Returns | Notes |
|---|---|---|
| `size` | Integer | Number of elements |
| `at: n` | Object | Element at 1-based index; signals `error` if out of range |
| `at: n Put: val` | `val` | Replaces element at index; signals `error` if out of range |
| `do: aBlock` | `self` | Evaluates `aBlock value: each` in order |
| `collect: aBlock` | Array | New array; each element is `aBlock value: each` |
| `select: aBlock` | OrderedCollection | Elements for which block is true |
| `detect: aBlock` | Object | First element for which block is true; signals `error` if none |
| `detect: aBlock IfNone: aDefault` | Object | Like `detect:`, evaluates `aDefault value` if none found |
| `inject: init Into: aBlock` | Object | Fold: `aBlock value: acc value: each` starting from `init` |
| `includes: obj` | Boolean | True if any element `= obj` |
| `with: other Collect: aBlock` | Array | Pairwise map; signals `error` if sizes differ |
| `copyFrom: start To: stop` | Array | Subarray, inclusive, 1-based; empty if start > stop |
| `reversed` | Array | New array with elements in reverse order |
| `asOrderedCollection` | OrderedCollection | Growable copy of this array |
| `printString` | String | Human-readable representation |

**Example:**

```
| a |
a: array new: 3.
a at: 1 Put: 'red'.
a at: 2 Put: 'green'.
a at: 3 Put: 'blue'.
a do: [:each | stdout println: each].
```

---

### OrderedCollection

`orderedCollection` — a growable, ordered sequence. Amortized O(1) append.

**Creating:**

```
orderedCollection new
```

Inherits all non-mutating array messages (`do:`, `collect:`, `select:`,
`detect:`, `detect:IfNone:`, `inject:Into:`, `includes:`, `copyFrom:To:`,
`reversed`). The messages below are additions or overrides.

**Messages:**

| Message | Returns | Notes |
|---|---|---|
| `size` | Integer | Current number of elements |
| `at: n` | Object | Element at 1-based index |
| `at: n Put: val` | `val` | Replace element at index |
| `add: obj` | `obj` | Append to end |
| `addFirst: obj` | `obj` | Prepend |
| `add: obj atIndex: n` | `obj` | Insert before element currently at n |
| `removeFirst` | Object | Remove and return first element; signals `error` if empty |
| `removeLast` | Object | Remove and return last element; signals `error` if empty |
| `remove: obj` | `obj` | Remove first occurrence by `=`; signals `error` if absent |
| `remove: obj IfAbsent: aBlock` | Object | Like `remove:`, evaluates block if absent |
| `first` | Object | First element without removing; signals `error` if empty |
| `last` | Object | Last element without removing; signals `error` if empty |
| `addAll: aCollection` | `aCollection` | Append all elements (any collection supporting `do:`) |
| `asArray` | Array | Fixed-size copy |
| `printString` | String | Human-readable representation |

---

### Dictionary

`dictionary` — an unordered collection of key-value associations. Any object
may be a key; equality is determined by `=`.

**Creating:**

```
dictionary new
```

**Messages:**

| Message | Returns | Notes |
|---|---|---|
| `size` | Integer | Number of associations |
| `at: key` | Object | Value for key; signals `error` if absent |
| `at: key IfAbsent: aBlock` | Object | Value for key, or `aBlock value` if absent |
| `at: key Put: val` | `val` | Associate key with val; adds if new, replaces if present |
| `includesKey: key` | Boolean | True if key is present |
| `keys` | OrderedCollection | All keys in unspecified order |
| `values` | OrderedCollection | All values, same order as `keys` |
| `do: aBlock` | `self` | `aBlock value: val` for each value |
| `keysAndValuesDo: aBlock` | `self` | `aBlock value: key value: val` for each pair |
| `removeKey: key` | Object | Remove pair, return removed value; signals `error` if absent |
| `removeKey: key IfAbsent: aBlock` | Object | Remove or evaluate block if absent |
| `copy` | Dictionary | Shallow copy |
| `printString` | String | Human-readable representation |

**Example:**

```
| d |
d: dictionary new.
d at: 'name' Put: 'Ada'.
d at: 'lang' Put: 'ego'.
stdout println: (d at: 'name').
d keysAndValuesDo: [:k :v |
    stdout show: k; show: ' => '; println: v
].
```

---

## I/O

### Console

The lobby binds `stdin`, `stdout`, and `stderr` as singleton stream objects.

**`stdout` and `stderr` messages:**

| Message | Returns | Notes |
|---|---|---|
| `print: obj` | `self` | Write `obj printString`; no trailing newline |
| `println: obj` | `self` | Like `print:` followed by a newline |
| `show: s` | `self` | Write string `s` directly (no `printString` conversion) |
| `nl` | `self` | Write a newline character |

**`stdin` messages:**

| Message | Returns | Notes |
|---|---|---|
| `readLine` | String or nil | Next line, trailing newline stripped; nil at EOF |
| `atEnd` | Boolean | True if EOF has been reached |

**Example:**

```
stdout show: 'Name: '.
| name |
name: stdin readLine.
name isNil
    ifTrue:  [stdout println: 'EOF reached']
    ifFalse: [stdout println: 'Hello, ' , name].
```

---

### File Streams

`fileStream` is a lobby prototype with factory messages for opening files.

**Opening a file:**

| Message | Returns | Notes |
|---|---|---|
| `fileStream read: path` | FileStream | Open for reading; signals `error` if not found |
| `fileStream write: path` | FileStream | Open for writing, create or truncate |
| `fileStream append: path` | FileStream | Open for appending, create if needed |

`path` is a string. The returned stream is a new object; the factory does not
modify `fileStream` itself.

**Read stream messages:**

| Message | Returns | Notes |
|---|---|---|
| `readLine` | String or nil | Next line, trailing newline stripped; nil at EOF |
| `readAll` | String | Remaining contents as one string |
| `atEnd` | Boolean | True if at end of file |
| `close` | `self` | Close the file; further reads signal `error` |

**Write stream messages:**

| Message | Returns | Notes |
|---|---|---|
| `show: s` | `self` | Write string `s` |
| `println: obj` | `self` | Write `obj printString` followed by newline |
| `nl` | `self` | Write newline |
| `close` | `self` | Flush and close; further writes signal `error` |

Both stream kinds support `isNil` → `false` (from the nil-testing convention
above) and `printString`.

**Example:**

```
| f |
f: fileStream read: 'input.txt'.
[f atEnd] whileFalse: [stdout println: f readLine].
f close.
```

---

## String Utilities

The core spec provides `,` (concatenation) and `printString`. These extend the
string protocol.

| Message | Returns | Notes |
|---|---|---|
| `size` | Integer | Number of Unicode codepoints |
| `at: n` | Integer | Codepoint at 1-based index; signals `error` if out of range |
| `copyFrom: start To: stop` | String | Substring, inclusive, 1-based; empty string if start > stop |
| `asUppercase` | String | ASCII case folding (locale-independent; full Unicode folding deferred) |
| `asLowercase` | String | ASCII case folding |
| `trimSeparators` | String | Strip leading and trailing whitespace (space, tab, CR, LF) |
| `startsWith: prefix` | Boolean | True if receiver begins with `prefix` |
| `endsWith: suffix` | Boolean | True if receiver ends with `suffix` |
| `includesSubstring: s` | Boolean | True if `s` appears anywhere in receiver |
| `indexOf: s` | Integer | 1-based index of first occurrence of `s`; 0 if not found |
| `= other` | Boolean | String equality (codepoint-by-codepoint) |
| `< other` | Boolean | Lexicographic order (codepoint values) |
| `asInteger` | Integer | Parse as decimal integer; signals `error` on failure |
| `asFloat` | Float | Parse as float; signals `error` on failure |

---

## Numeric Utilities

The core spec provides `+ - * /`, comparison (`< > <= >= = ~=`), and
transparent bignum promotion. These extend the numeric protocols.

### Both Integer and Float

| Message | Returns | Notes |
|---|---|---|
| `abs` | Same type | Absolute value |
| `max: n` | Number | Larger of receiver and `n` |
| `min: n` | Number | Smaller of receiver and `n` |
| `sqrt` | Float | Square root; always returns float |

### Integer Only

| Message | Returns | Notes |
|---|---|---|
| `// n` | Integer | Floor division (toward −∞); signals `zeroDivide` if n = 0 |
| `% n` | Integer | Remainder consistent with `//`; non-negative when n > 0; signals `zeroDivide` if n = 0 |
| `raisedTo: n` | Integer | Integer exponentiation for n ≥ 0; signals `error` for n < 0 |
| `asFloat` | Float | Convert to float |
| `printString: base` | String | Representation in given base (2–36) |

Note: `/` on two integers returns a float (IEEE division). Use `//` for
integer division.

### Float Only

| Message | Returns | Notes |
|---|---|---|
| `floor` | Integer | Round toward −∞ |
| `ceiling` | Integer | Round toward +∞ |
| `truncated` | Integer | Round toward zero |
| `rounded` | Integer | Round to nearest integer (ties toward even) |
| `raisedTo: n` | Float | Exponentiation |
| `asInteger` | Integer | Same as `truncated` |
| `isNaN` | Boolean | True if IEEE NaN |
| `isInfinite` | Boolean | True if IEEE infinity |

### Math Object

`math` is a lobby object providing numeric constants and transcendental functions.

| Message / slot | Returns | Notes |
|---|---|---|
| `math pi` | Float | π |
| `math e` | Float | e |
| `math sin: x` | Float | Sine (radians) |
| `math cos: x` | Float | Cosine (radians) |
| `math tan: x` | Float | Tangent (radians) |
| `math ln: x` | Float | Natural logarithm; signals `error` if x ≤ 0 |
| `math log: x` | Float | Base-10 logarithm; signals `error` if x ≤ 0 |
| `math exp: x` | Float | eˣ |

---

## Lobby Bindings Summary

The following names are bound in the lobby in addition to those defined in
`lang-spec.md` §6 and §8:

| Name | Object |
|---|---|
| `array` | Array prototype |
| `orderedCollection` | OrderedCollection prototype |
| `dictionary` | Dictionary prototype |
| `stdin` | Standard input stream |
| `stdout` | Standard output stream |
| `stderr` | Standard error stream |
| `fileStream` | File stream prototype (factory messages) |
| `math` | Math object |
