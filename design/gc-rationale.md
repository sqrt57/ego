# GC algorithm rationale

Rationale for choosing mark-and-sweep for Stage 1 over other collection strategies.

## Decision

Stop-the-world mark-and-sweep. No compaction. Details in
[rs-treewalk-impl.md](rs-treewalk-impl.md#gc).

## Why not stop-and-copy (semispace)?

Semispace gives bump-pointer allocation and fast collection (touches only live
objects), but requires updating every reference after copying. In ego, `ObjectId`
is a plain `u32` index into the arena `Vec`. When an object moves from index 5 to
index 3, every `ObjectId(5)` anywhere in the system becomes a dangling reference.

References are held in:

- `Slot::value` inside every live object
- `RootSet` fields
- `Env` frames (lexical scope), including frames on the Rust call stack
- `BlockData` captures (closures)
- Rust local variables in the evaluator

The standard fix is forwarding pointers: overwrite the old slot with a "moved to
N" marker, then do a second pass over all roots and slots to rewrite stale
indices. This requires an exhaustive, correct scan of every `ObjectId` source —
miss one and you have silent memory corruption. It also requires either writing
back into the evaluator's Rust stack frames or re-reading from `stack_roots` after
every GC point. Significant plumbing for no benefit in a tree-walker.

## Why not mark-compact?

Same reference-update problem as semispace, but without the 2× memory cost.
Still more complex than mark-sweep, still requires forwarding pointers or
equivalent. Not worth it unless memory is very constrained.

## Why not reference counting?

Ego's prototype chains are cycles by definition (an object's parent is another
object, which may have a parent back up the chain). Reference counting cannot
collect cycles without an additional cycle-detection pass (e.g. Bacon–Rajan
trial deletion), which is more complex than mark-sweep to begin with.

## Why not generational?

Generational collection layers on top of a base algorithm and requires a write
barrier to track old→young pointers. There's no point building it before the
base collector is solid and before there's evidence that allocation pressure
warrants it.

## Fragmentation

Mark-and-sweep over this arena does not suffer classical fragmentation. Every
entry in the `Vec<Object>` is the same size — the variable-sized payloads
(`Vec<Slot>`, `String`, `Box<MethodDef>`, etc.) are heap-allocated separately and
managed by the system allocator. Every free slot on the arena free list can
accommodate any new object regardless of content. There are no size-class
mismatches.

What does accumulate is **scattered live objects** — after many alloc/free cycles
the live set is non-contiguous in the Vec, which hurts cache locality on GC
traversal. For a tree-walker this is acceptable. A bytecode VM (Stage 2+) cares
more, which is why the design defers a moving collector to Stage 3.

## Stage 3

When the Zig VM is built, a moving collector becomes worthwhile. By then the
reference-update infrastructure (forwarding pointers, exhaustive root scanning)
is worth building for the throughput gain from bump allocation and improved
locality.
