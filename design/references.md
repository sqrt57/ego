# References

Sources for the Self documentation review (`self-notes.md`) and ongoing ego design work.

Downloaded PDFs are in `../papers/` (not tracked in git).

---

## Self Language Reference

**Self Handbook (2024.1)**
— Official language reference. Covers the full object model, message dispatch, blocks, mirrors, exception handling, cascades, and the numeric tower.
— https://handbook.selflanguage.org/2024.1/ (online only, no PDF)
— Chapter 4, "The Self World" (built-in/stdlib objects), is the primary source for `self-notes.md` §12 and `stdlib.md`: `selfwrld.html`, with 15 subsections at the same base path — `worldorg.html` (world organization/lobby), `roots.html` (default behavior, clonable/oddball), `blocks.html` (blocks/booleans/control), `numbers.html` (numeric tower, time), `collections.html`, `pairs.html`, `mirrors.html`, `messages.html` (message objects, resend/delegation), `processes.html`, `foreign.html` (FFI), `unix.html` (I/O), `oddball.html` (misc singletons), `lowlevel.html` (interrupts), `textdebug.html` (textual debugger), `logging.html`.

---

## Foundational Papers

**"Self: The Power of Simplicity"**
— David Ungar, Randall B. Smith. OOPSLA 1987.
— The original design paper. Rationale for prototype-based objects, no classes, slots as the uniform abstraction.
— `../papers/self-power.pdf`

**"An Efficient Implementation of Self"**
— Craig Chambers, David Ungar, Elgin Lee. OOPSLA 1989.
— Maps (hidden classes), polymorphic inline caches, type feedback. The implementation model behind Self's performance; directly relevant to ego's VM stages.
— `../papers/implementation.pdf`

**"Organizing Programs Without Classes"**
— David Ungar, Craig Chambers, Bay-Wei Chang, Urs Hölzle. Lisp and Symbolic Computation 1991.
— Traits/prototype split pattern — how to structure a classless object system in practice. The idiom ego programs will follow.
— `../papers/organizing-programs.pdf`

**"Parents are Shared Parts: Inheritance and Encapsulation in Self"**
— David Ungar et al.
— How parent slots serve as the mechanism for both inheritance and encapsulation.
— `../papers/parents-shared-parts.pdf`

**"Programming as an Experience: The Inspiration for Self"**
— Randall B. Smith, David Ungar. ECOOP 1995.
— Retrospective on the design philosophy and motivations behind Self.
— `../papers/programming-as-experience.pdf`

**"Making Pure Object-Oriented Languages Practical"**
— Craig Chambers, David Ungar. OOPSLA 1991.
— How to make a prototype-based language fast enough for real use; optimization strategies.
— `../papers/practical.pdf`

**"A Third-Generation Self Implementation: Reconciling Responsiveness with Performance"**
— Urs Hölzle, David Ungar. OOPSLA 1994.
— Deoptimization and adaptive compilation; relevant background for ego's Stage 3 Zig VM.
— `../papers/third-generation.pdf`

---

## Mirror-Based Reflection

**"Mirrors: Design Principles for Meta-level Facilities of Object-Oriented Programming Languages"**
— Gilad Bracha, David Ungar. OOPSLA 2004.
— Canonical paper on the mirror API design. Motivates why reflection should be separated from the base object model.
— `../papers/mirrors.pdf`

---

## VM and GC Background

**"The Design and Implementation of the Self Compiler"**
— Craig Chambers. PhD dissertation, Stanford 1992.
— Deep coverage of type feedback, inlining, and the Self JIT. Background for ego's Stage 3 (Zig VM) optimization work.
— `../papers/chambers-dissertation.pdf`

---

## Smalltalk Background

**"Smalltalk-80: The Language and Its Implementation"**
— Adele Goldberg, David Robson. Addison-Wesley 1983.
— The "Blue Book." Self's block/closure semantics and control-flow-via-messages convention derive directly from Smalltalk-80; ego inherits them.
— `../papers/smalltalk-80-blue-book.pdf`

**"Design Principles Behind Smalltalk"**
— Daniel H. H. Ingalls. BYTE Magazine 1981.
— Short and readable statement of the design philosophy ego most closely follows.
— https://www.cs.virginia.edu/~evans/cs655/readings/smalltalk.html
