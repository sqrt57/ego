# References

Sources for the Self documentation review (`self-notes.md`) and ongoing ego design work.

Downloaded PDFs are in `../sources/` (sibling of both repos).

---

## Self Language Reference

**Self Handbook (2024.1)**
— Official language reference. Covers the full object model, message dispatch, blocks, mirrors, exception handling, cascades, and the numeric tower.
— https://handbook.selflanguage.org/2024.1/ (online only, no PDF)

---

## Foundational Papers

**"Self: The Power of Simplicity"**
— David Ungar, Randall B. Smith. OOPSLA 1987.
— The original design paper. Rationale for prototype-based objects, no classes, slots as the uniform abstraction.
— `../sources/self-power.pdf`

**"An Efficient Implementation of Self"**
— Craig Chambers, David Ungar, Elgin Lee. OOPSLA 1989.
— Maps (hidden classes), polymorphic inline caches, type feedback. The implementation model behind Self's performance; directly relevant to ego's VM stages.
— `../sources/implementation.pdf`

**"Organizing Programs Without Classes"**
— David Ungar, Craig Chambers, Bay-Wei Chang, Urs Hölzle. Lisp and Symbolic Computation 1991.
— Traits/prototype split pattern — how to structure a classless object system in practice. The idiom ego programs will follow.
— `../sources/organizing-programs.pdf`

**"Parents are Shared Parts: Inheritance and Encapsulation in Self"**
— David Ungar et al.
— How parent slots serve as the mechanism for both inheritance and encapsulation.
— `../sources/parents-shared-parts.pdf`

**"Programming as an Experience: The Inspiration for Self"**
— Randall B. Smith, David Ungar. ECOOP 1995.
— Retrospective on the design philosophy and motivations behind Self.
— `../sources/programming-as-experience.pdf`

**"Making Pure Object-Oriented Languages Practical"**
— Craig Chambers, David Ungar. OOPSLA 1991.
— How to make a prototype-based language fast enough for real use; optimization strategies.
— `../sources/practical.pdf`

**"A Third-Generation Self Implementation: Reconciling Responsiveness with Performance"**
— Urs Hölzle, David Ungar. OOPSLA 1994.
— Deoptimization and adaptive compilation; relevant background for ego's Stage 3 Zig VM.
— `../sources/third-generation.pdf`

---

## Mirror-Based Reflection

**"Mirrors: Design Principles for Meta-level Facilities of Object-Oriented Programming Languages"**
— Gilad Bracha, David Ungar. OOPSLA 2004.
— Canonical paper on the mirror API design. Motivates why reflection should be separated from the base object model.
— `../sources/mirrors.pdf`

---

## VM and GC Background

**"The Design and Implementation of the Self Compiler"**
— Craig Chambers. PhD dissertation, Stanford 1992.
— Deep coverage of type feedback, inlining, and the Self JIT. Background for ego's Stage 3 (Zig VM) optimization work.
— `../sources/chambers-dissertation.pdf`

---

## Smalltalk Background

**"Smalltalk-80: The Language and Its Implementation"**
— Adele Goldberg, David Robson. Addison-Wesley 1983.
— The "Blue Book." Self's block/closure semantics and control-flow-via-messages convention derive directly from Smalltalk-80; ego inherits them.
— `../sources/smalltalk-80-blue-book.pdf`

**"Design Principles Behind Smalltalk"**
— Daniel H. H. Ingalls. BYTE Magazine 1981.
— Short and readable statement of the design philosophy ego most closely follows.
— https://www.cs.virginia.edu/~evans/cs655/readings/smalltalk.html
