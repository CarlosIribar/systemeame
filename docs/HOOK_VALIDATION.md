# Validate a query operation once

Treat `Database.getQueryLocator([SELECT ...], optionalAccessLevel)` and its inline
query as **one operation**, with two alternative places to specify its mode.

| Query clause | Call argument | Result |
| --- | --- | --- |
| Missing | Missing | Missing mode; propose one location only |
| USER/SYSTEM | Missing | Valid; do not request another argument |
| Missing | USER/SYSTEM | Valid; do not request an inner WITH |
| Missing | AccessLevel variable/expression | Explicit call policy; do not append a mode |
| USER/SYSTEM | Same mode | Duplicate; remove the optional call argument |
| USER/SYSTEM | Different/unknown mode | Conflict requiring review; do not choose a policy automatically |

Associate the **direct first argument** with its enclosing Database call. Do not
search the whole call for a mode: modes in nested calls, bind expressions, strings,
comments, or unrelated queries do not apply to the outer operation. Parentheses,
multiline formatting, casing, and comments between tokens must not change results.
Subqueries inherit their outer query's policy. An independently evaluated query in
a bind expression remains a separate operation.

For dynamic queries, inspect only the query argument and the overload's access-level
position. Parse resolved string literals/concatenations as SOQL; words in quoted
WHERE values are not clauses. If runtime text is unknown, report that its policy
cannot be verified; never claim it is missing or instruct someone to blindly append
SYSTEM_MODE. `*WithBinds` requires a mode argument: do not fix a duplicate by deleting
that required argument.

## Renovo integration points

The locally available `origin/query-system-mode` version of
`.scripts/check-apex-system-mode.js` checks bracketed queries in
`findStaticQueryViolations` and separately skips inline arguments in
`findDynamicQueryViolations`. The static pass does not associate a query with an
existing access-level argument in its enclosing call. Consequently, an inline
query without WITH can be incorrectly rejected even when the call has a mode.

Build a shared operation list (prefer an Apex parser) before running either check.
Record the query span, enclosing call, direct arguments, and mode location. Validate
each operation once using the table above. Do not merely skip every query inside a
Database call: that would accept operations with no mode anywhere. Detect duplicate
and conflicting declarations explicitly, with messages asking to remove/reconcile
modes rather than add one.

Keep existing project exclusions and extend the hook tests with every table row,
comments, case variations, parentheses, nested queries, and mode variables. Validate
the Git index contents being committed, including `.trigger` files, rather than
reading unstaged working-tree changes as if they were staged.

The plugin's `--check` uses the same analysis as `fix` and recognizes these exclusive
mode locations. It is not a drop-in replacement for all Renovo checks: project
ignore directives and SOSL rewriting are not implemented, and its default staged
scope rejects partially staged files rather than parsing index blobs.
