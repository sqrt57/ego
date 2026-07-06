# ego Command-Line Interface

## Synopsis

```
ego [options] [<file> ...]
```

`<file>` arguments are `.ego` source files.

## Options

| Option | Description |
|---|---|
| `-e <code>`, `--eval <code>` | Evaluate `<code>` as a program fragment. May be given multiple times. |
| `--repl` | Start an interactive REPL. |
| `--version` | Print version and exit. |
| `--help` | Print usage and exit. |

## Modes

### Script

```
ego program.ego
```

Reads the file, evaluates it top to bottom as a single [Program](lang-grammar.md#program)
against the lobby, and exits. Only the exit code communicates success or
failure — no result is printed unless the program itself prints one.

### Mixed eval and files

```
ego -e "code" file1.ego -e "more code" file2.ego
```

Each `-e` / `--eval` argument and each `<file>` argument is a program fragment.
Fragments are executed consecutively in the order they appear on the command
line, all against the same lobby. Only the exit code communicates success or
failure — no result is printed unless the program itself prints one.

### REPL

```
ego --repl
```

Reads one expression at a time, evaluates it against the lobby, and prints
the resulting object's `printString` before reading the next one. State
persists across expressions within a session — a `var` slot declared in one
line is visible in the next.

Prompt:

```
ego> 3 + 4
7
ego> 'hi' , ' there'
'hi there'
```

### Inline eval

```
ego -e "3 + 4"
ego --eval "3 + 4"
```

Equivalent to a one-line script; prints the result of the final expression
and exits.

## Diagnostics

Errors are written to stderr:

```
path/to/file.ego:3:12: error: message text
```

Line and column numbers are 1-based. REPL errors omit the file path.

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Success |
| `1` | Uncaught runtime error |
| `2` | Bad arguments (includes running with no arguments) |

## Examples

Run a script:
```
ego hello.ego
```

Start the REPL:
```
ego --repl
```

One-off evaluation, useful for scripting/CI:
```
ego -e "(2 + 2) printString"
ego --eval "(2 + 2) printString"
```

Mix inline code and files, executed in order:
```
ego -e "x := 1" setup.ego -e "x printString"
```
