; From Helix (https://github.com/helix-editor/helix), commit 872b240251876e56787ad7ec312cefae70e3e28e,
; runtime/queries/php/injections.scm. Licensed under the Mozilla Public
; License 2.0 (https://mozilla.org/MPL/2.0/). Changed by Ahab: inherited
; queries are written out in place of their `; inherits:` lines, and
; the ones apps/ui/languages/sources.json adds (`inherit`) come first.
((text) @injection.content
 (#set! injection.language "html")
 (#set! injection.combined))

((comment) @injection.content
 (#set! injection.language "comment"))

((function_call_expression
 function: (name) @_function
 arguments: (arguments . (argument (_ (string_content) @injection.content))))
 (#match? @_function "^preg_")
 (#set! injection.language "regex"))

((function_call_expression
 function: (name) @_function
 arguments: (arguments (_) (argument (_ (string_content) @injection.content))))
 (#match? @_function "^mysqli_")
 (#set! injection.language "sql"))

((member_call_expression
 object: (_)
 name: (name) @_function
 arguments: (arguments . (argument (_ (string_content) @injection.content))))
 (#match? @_function "^(prepare|query)$")
 (#set! injection.language "sql"))
