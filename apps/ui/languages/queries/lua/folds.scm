; From Helix (https://github.com/helix-editor/helix), commit 872b240251876e56787ad7ec312cefae70e3e28e,
; runtime/queries/lua/folds.scm. Licensed under the Mozilla Public
; License 2.0 (https://mozilla.org/MPL/2.0/). Changed by Ahab: inherited
; queries are written out in place of their `; inherits:` lines, and
; the ones apps/ui/languages/sources.json adds (`inherit`) come first.
[
 (do_statement)
 (while_statement)
 (repeat_statement)
 (if_statement)
 (for_statement)
 (function_declaration)
 (function_definition)
 (table_constructor)
] @fold
