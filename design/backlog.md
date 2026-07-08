# ego Design Backlog

Open questions and known small inconsistencies, deliberately deferred rather
than blocking the substage that surfaced them. Resolve or fold into the
relevant design doc when the roadmap reaches the area again.

## Ambiguity-report format for message-not-understood

`self-notes.md` §4 (lookup algorithm) says an ambiguous lookup (a selector
reachable through more than one parent) should "signal a
message-not-understood with an ambiguity report," without specifying what
the report should contain. Substage 1.9's implementation
(`lookup_in_parents` in `eval.rs`) signals ambiguity with a plain one-line
message naming the selector, with no enumeration of the competing parent
paths. Revisit once exception handling (substage 1.15) defines a real
error-object shape — an ambiguity report likely wants to carry the list of
parents/paths that matched, not just prose.
