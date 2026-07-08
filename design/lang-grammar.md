# ego Formal Grammar

Grammar for ego, in Go-style EBNF.
See [lang-spec.md](lang-spec.md) for the full language specification.

---

## Notation

Each production has the form:

    Rule = expression .

| Operator | Meaning |
|---|---|
| `\|` | alternation |
| `( )` | grouping |
| `[ ]` | zero or one |
| `{ }` | zero or more |
| `"text"` | literal terminal |
| `` `text` `` | literal terminal containing `"` |
| `a … b` | any character in the inclusive range a through b |
| `/* prose */` | terminal described informally |

---

## Lexical Grammar

Source files are UTF-8 encoded. Whitespace (U+0020, U+0009, U+000D, U+000A)
and comments are skipped between tokens.

### Characters

    unicode_char  = /* any Unicode scalar value */ .
    small_letter  = "a" … "z" | "_" .
    cap_letter    = "A" … "Z" .
    letter        = small_letter | cap_letter .
    decimal_digit = "0" … "9" .
    general_digit = decimal_digit | small_letter | cap_letter .

### Comments

    comment = `"` { comment_char | `""` } `"` .
    comment_char = /* any unicode_char except `"` */ .

Comments are not nested. `""` inside a comment is a literal `"` character.

### Identifiers

    identifier = small_letter { letter | decimal_digit } .

Identifiers must begin with a lowercase letter or `_`. `self` and `resend`
are reserved pseudo-variables and may not be used as slot names or block
parameters. `true`, `false`, and `nil` are **not** reserved; they are
ordinary identifiers bound in the lobby.

### Selectors

    unary_selector   = identifier .              (* not immediately followed by ":" *)
    keyword_part     = identifier ":" .          (* lowercase-start keyword part *)
    cap_keyword_part = cap_letter { letter | decimal_digit } ":" .
    binary_char      = "+" | "-" | "*" | "/" | "~" | "<" | ">"
                      | "=" | "&" | "@" | "%" | "," | "?" | "!" .
    binary_selector  = binary_char { binary_char } .

`|` and `^` are excluded from `binary_char`: `|` delimits slot lists in
object and block literals (`(| ... |)`, `[| ... |]`), and `^` marks a
return statement. Allowing either into a binary selector would make the
lexer ambiguous at those boundaries.

### Integer Literals

    integer_lit = [ "-" ] decimal_digit { decimal_digit }
                  [ ( "r" | "R" ) general_digit { general_digit } ] .

A leading `-` is part of the literal (ego has no unary minus message). When a
base prefix is present (e.g. `16rFF`, `8r17`, `2r1010`), the digits before
`r`/`R` are the base in decimal and the digits after are the value written in
that base using digits and lowercase/uppercase letters as needed.

Because the lexer is context-free, `-` immediately followed by a decimal digit
is always tokenised as the start of a negative literal. Consequently `a -3`
lexes as `[Ident("a"), Integer(-3)]` — two adjacent primaries — which no
syntactic production accepts. Subtraction must be written `a - 3` (with a
space after `-`).

### Float Literals

    float_lit = [ "-" ] decimal_digit { decimal_digit } "."
                decimal_digit { decimal_digit }
                [ ( "e" | "E" ) [ "-" ] decimal_digit { decimal_digit } ] .

At least one digit is required on each side of `.`.

### String Literals

    string_lit     = "'" { string_char | escape_seq | "''" | continuation } "'" .
    string_char    = /* any unicode_char except "'" and "\" */ .
    continuation   = "\" "\n" .                  (* escaped newline; contributes no character *)
    escape_seq     = "\" ( "t" | "b" | "n" | "f" | "r" | "v" | "a" | "0"
                          | "\" | "'" | `"` | "?"
                          | "x" hex_digit hex_digit
                          | "d" decimal_digit decimal_digit decimal_digit
                          | "o" oct_digit oct_digit oct_digit ) .
    hex_digit      = decimal_digit | "a" … "f" | "A" … "F" .
    oct_digit      = "0" … "7" .

`''` inside a string is a literal `'` character, kept for Smalltalk-style
quoting alongside the backslash escapes. `\t \b \n \f \r \v \a \0` are the
usual control-character escapes; `\\`, `\'`, `\"`, `\?` escape themselves;
`\xHH`, `\dDDD`, `\oOOO` give a character by hex/decimal/octal code
respectively (carriage return can be written `\r`, `\x0d`, `\d013`, or
`\o015`). A `\` immediately followed by a newline is a **line
continuation** — both the backslash and the newline are dropped, letting a
string literal span multiple source lines without embedding the newline
itself.

---

## Syntactic Grammar

### Program

    Program = [ Stmt { "." Stmt } [ "." ] ] .

A program is a sequence of statements evaluated against the lobby, in order.

### Statements

    Stmt = "^" CascadeExpr | CascadeExpr .

`^` returns the value of `CascadeExpr` from the enclosing method or block body. At
top level (outside any method or block) its behavior is unspecified.

### Cascades

    CascadeExpr = Expr [ ";" CascadeMsg { ";" CascadeMsg } ] .
    CascadeMsg  = unary_selector
                | binary_selector UnaryExpr
                | ( keyword_part | cap_keyword_part ) BinaryExpr
                  { ( keyword_part | cap_keyword_part ) BinaryExpr }
                .

See [lang-spec.md §9](lang-spec.md#9-cascades) for semantics.

### Expressions

Precedence, loosest to tightest: keyword, binary, unary.

    Expr        = KeywordExpr | BinaryExpr .
    KeywordExpr = [ BinaryExpr ] ( keyword_part | cap_keyword_part ) BinaryExpr
                  { ( keyword_part | cap_keyword_part ) BinaryExpr }
                | BinaryExpr
                .
    BinaryExpr  = [ UnaryExpr ] binary_selector UnaryExpr { binary_selector UnaryExpr }
                | UnaryExpr
                .
    UnaryExpr   = Primary { unary_selector } .

The leading `BinaryExpr`/`UnaryExpr` may be omitted from a `KeywordExpr` or
`BinaryExpr` — an **implicit-receiver send**, meaning "send to `self`":
`min: 5` alone means `self min: 5`, and `+ 3` alone means `self + 3`. A bare
`unary_selector` needs no such alternative in the grammar, since a plain
`identifier` is already a `Primary`; whether it resolves to a local
variable/parameter or to an implicit unary send to `self` is a lookup-time
distinction, not a parse-time one — see
[lang-spec.md §2](lang-spec.md#2-messages).

Repeated `binary_selector` tokens within one `BinaryExpr` must all be
identical — this isn't expressible in the BNF above, so it's checked as a
parse-time constraint. `3 + 4 + 7` is valid (repeats `+`); `3 + 4 * 7` is a
syntax error (mixes `+` and `*`) and must be parenthesized: `(3 + 4) * 7` or
`3 + (4 * 7)`. See [lang-spec.md §2](lang-spec.md#2-messages).

The first `( keyword_part | cap_keyword_part )` consumed by a `KeywordExpr`
must be a plain `keyword_part` (lowercase-initial) — a `cap_keyword_part`
may only continue a keyword message already in progress, never start one.
This, together with how consecutive parts group into possibly-nested
sends, is a semantic constraint layered on top of the flat token sequence
above; see [lang-spec.md §2](lang-spec.md#2-messages) for the grouping
algorithm and worked examples.

    Primary = integer_lit
            | float_lit
            | string_lit
            | identifier
            | Assign
            | "self"
            | ResendExpr
            | "(" Expr ")"
            | ObjectLiteral
            | BlockLiteral
            .

### Local assignment

    Assign = identifier "<-" Expr .

Reassigns a local variable (a block/method parameter or a `<-`-declared
local) already reachable in the enclosing lexical scope — see
[lang-spec.md §3](lang-spec.md#3-blocks). Distinct from `VarSlotDecl`, which
uses the same `identifier "<-" Expr` shape but only inside a `| … |`
slot-decl header and declares an object *slot* instead. Because `Assign` is
recognized wherever a `Primary` is expected, `identifier "<-" Expr` written
anywhere else — a body statement, a keyword-send argument, inside parens —
reassigns rather than sends a `<-` message; sending `<-` as an ordinary
binary selector is only reachable when the receiver isn't a bare identifier
(`(a foo) <- b`), which no built-in object defines a method for.

### Resends

    ResendExpr = ResendTarget "." ResendMessage .
    ResendTarget = "resend" | identifier .
    ResendMessage = unary_selector
                  | binary_selector UnaryExpr
                  | ( keyword_part | cap_keyword_part ) BinaryExpr
                    { ( keyword_part | cap_keyword_part ) BinaryExpr }
                  .

No whitespace may separate `ResendTarget`, `"."`, and the start of
`ResendMessage` — `resend.display`, `resend.+ 5`, `resend.min: 17 Max: 23`.
`"resend"` is an **undirected resend**: lookup continues from the parent
chain of the object that defined the currently executing method, in the
usual depth-first left-to-right order. An `identifier` naming one of that
object's own parent slots is a **directed resend**: lookup is constrained
to that one parent slot's value and its own ancestors, resolving
ambiguity when a method is reachable through more than one parent. See
[lang-spec.md §2](lang-spec.md#2-messages).

A cap-initial keyword part continues the *same* selector as the part before
it: `at: 1 Put: 2` is a single send of `at:Put:` with two arguments, not two
sends. A lowercase-initial keyword part after the first, by contrast, ends
the current message and starts a **new, nested** one, right-associatively
— see [lang-spec.md §2](lang-spec.md#2-messages) for the full grouping
rule and worked examples.

### Object Literals

    ObjectLiteral = "(" "|" [ Annotation ] [ SlotList ] "|" [ Code ] ")" .
    SlotList      = SlotDecl { "." SlotDecl } [ "." ] .
    Code          = Stmt { "." Stmt } [ "." ] .
    Annotation    = "{" "}" "=" string_lit "." .

    SlotDecl       = DataSlotDecl
                   | VarSlotDecl
                   | ArgSlotDecl
                   | ParentSlotDecl
                   | MethodSlotDecl
                   .
    DataSlotDecl   = identifier "=" Expr .
    VarSlotDecl    = identifier "<-" Expr .
    ArgSlotDecl    = ":" identifier .
    ParentSlotDecl = identifier "*" "=" Expr .
    MethodSlotDecl = MethodSelector "=" "(" [ Code ] ")" .

The `"("` in `MethodSlotDecl` is the method body delimiter, not the start of
a parenthesised expression. Consequently `x = (…)` inside a slot list is
**always** parsed as a unary method slot, never as a data slot whose value
happens to be wrapped in parens. Data slots with complex expressions must
omit the outer parens: `x = a + b`, not `x = (a + b)`.

An object literal `(| … |)` is unambiguous as a data slot value because the
`(` is immediately followed by `|`, which signals an object literal primary
rather than a method body.

In `ParentSlotDecl`, the `"*"` and `"="` must be separate tokens (i.e.
separated by whitespace). Writing `p*=` produces a single `Binary("*=")` token
and is a syntax error; write `p* = val` instead.

    MethodSelector = identifier                                                (* unary *)
                   | binary_selector identifier                                (* binary, one param *)
                   | ( keyword_part | cap_keyword_part ) identifier
                     { ( keyword_part | cap_keyword_part ) identifier }       (* keyword *)
                   .

An `ArgSlotDecl` declares a parameter slot directly inside an object literal,
making it a standalone method object: `(| :x :y | x + y)`. The more common
way to define method parameters is via the `MethodSelector` in
`MethodSlotDecl`, which binds each keyword part to its parameter name
explicitly.

### Block Literals

    BlockLiteral  = "[" [ "|" [ BlockSlotList ] "|" ] [ Code ] "]" .
    BlockSlotList = BlockSlotDecl { "." BlockSlotDecl } [ "." ] .
    BlockSlotDecl = ArgSlotDecl | DataSlotDecl | VarSlotDecl .

Arg slots (`:name`) declare block parameters. Data and var slots declare
block-local variables. All slot declarations appear between `|` delimiters
before the block body.
