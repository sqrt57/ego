# Repo structure

Single git repo at `github.com/sqrt57/ego`.

## Layout

```
ego/
  Cargo.toml      ← Rust workspace (Stages 1–2)
  common/         ← shared lib: lexer, parser, arena, GC
  treewalk/       ← Stage 1: tree-walking interpreter
  vm/             ← Stage 2: bytecode compiler + VM
  zig-vm/         ← Stage 3: Zig VM
    build.zig
    src/
  boot/
    boot.ego      ← standard library bootstrap, embedded at compile time
  design/         ← design docs (this folder)
  papers/         ← academic reference PDFs, not tracked in git
```

## Branching

- Source code changes: feature branch → merge to `main`
- Doc changes coupled to a code change: same feature branch
- Standalone doc changes: commit directly to `main`

## Licenses

MIT for all tracked content. `papers/` is not tracked — third-party copyright.
