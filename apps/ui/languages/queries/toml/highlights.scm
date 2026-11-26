; From Helix (https://github.com/helix-editor/helix), commit 872b240251876e56787ad7ec312cefae70e3e28e,
; runtime/queries/toml/highlights.scm. Licensed under the Mozilla Public
; License 2.0 (https://mozilla.org/MPL/2.0/). Changed by Ahab: inherited
; queries are written out in place of their `; inherits:` lines, and
; the ones apps/ui/languages/sources.json adds (`inherit`) come first.
; Properties
;-----------

(table [
  (bare_key)
  (dotted_key)
  (quoted_key)
] @type)

(table_array_element [
  (bare_key)
  (dotted_key)
  (quoted_key)
] @type)

(pair [
  (bare_key)
  (dotted_key)
  (quoted_key)
] @variable.other.member)

; Literals
;---------

(boolean) @constant.builtin.boolean
(comment) @comment
(string) @string
(integer) @constant.numeric.integer
(float) @constant.numeric.float
(offset_date_time) @string.special
(local_date_time) @string.special
(local_date) @string.special
(local_time) @string.special

; Punctuation
;------------

"." @punctuation.delimiter
"," @punctuation.delimiter

"=" @operator

"[" @punctuation.bracket
"]" @punctuation.bracket
"[[" @punctuation.bracket
"]]" @punctuation.bracket
"{" @punctuation.bracket
"}" @punctuation.bracket
