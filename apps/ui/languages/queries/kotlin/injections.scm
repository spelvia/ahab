; From Helix (https://github.com/helix-editor/helix), commit 872b240251876e56787ad7ec312cefae70e3e28e,
; runtime/queries/kotlin/injections.scm. Licensed under the Mozilla Public
; License 2.0 (https://mozilla.org/MPL/2.0/). Changed by Ahab: inherited
; queries are written out in place of their `; inherits:` lines, and
; the ones apps/ui/languages/sources.json adds (`inherit`) come first.
([
  (line_comment)
  (multiline_comment)
 ] @injection.content
	(#set! injection.language "comment"))

; There are 3 ways to define a regex
;    - "[abc]?".toRegex()
((call_expression
	(navigation_expression
		(string_literal
		   (string_content) @injection.content)
		(navigation_suffix
			((simple_identifier) @_function
			(#eq? @_function "toRegex")))))
	(#set! injection.language "regex"))

;    - Regex("[abc]?")
((call_expression
	((simple_identifier) @_function
	(#eq? @_function "Regex"))
	(call_suffix
		(value_arguments
			(value_argument
				(string_literal
					(string_content) @injection.content)))))
	(#set! injection.language "regex"))

;    - Regex.fromLiteral("[abc]?")
((call_expression
	(navigation_expression
		((simple_identifier) @_class
		(#eq? @_class "Regex"))
		(navigation_suffix
			((simple_identifier) @_function
			(#eq? @_function "fromLiteral"))))
	(call_suffix
		(value_arguments
			(value_argument
				(string_literal
					(string_content) @injection.content)))))
	(#set! injection.language "regex"))
