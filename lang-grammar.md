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
                      | "=" | "&" | "|" | "@" | "%" | "," | "?" | "!" .
    binary_selector  = binary_char { binary_char } .

### Integer Literals

    integer_lit = [ "-" ] decimal_digit { decimal_digit }
                  [ ( "r" | "R" ) general_digit { general_digit } ] .

A leading `-` is part of the literal (ego has no unary minus message). When a
base prefix is present (e.g. `16rFF`, `8r17`, `2r1010`), the digits before
`r`/`R` are the base in decimal and the digits after are the value written in
that base using digits and lowercase/uppercase letters as needed.

### Float Literals

    float_lit = [ "-" ] decimal_digit { decimal_digit } "."
                decimal_digit { decimal_digit }
                [ ( "e" | "E" ) [ "-" ] decimal_digit { decimal_digit } ] .

At least one digit is required on each side of `.`.

### String Literals

    string_lit  = "'" { string_char | "''" } "'" .
    string_char = /* any unicode_char except "'" */ .

`''` inside a string is a literal `'` character.

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
    KeywordExpr = BinaryExpr { ( keyword_part | cap_keyword_part ) BinaryExpr } .
    BinaryExpr  = UnaryExpr { binary_selector UnaryExpr } .
    UnaryExpr   = Primary { unary_selector } .

    Primary = integer_lit
            | float_lit
            | string_lit
            | identifier
            | "self"
            | "resend"
            | "(" Expr ")"
            | ObjectLiteral
            | BlockLiteral
            .

Consecutive keyword parts — small or cap — accumulate into one selector and
argument list: `at: 1 Put: 2` is a single send of `at:Put:` with two
arguments, not two sends.

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
