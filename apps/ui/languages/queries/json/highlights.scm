; From Helix (https://github.com/helix-editor/helix), commit 872b240251876e56787ad7ec312cefae70e3e28e,
; runtime/queries/json/highlights.scm. Licensed under the Mozilla Public
; License 2.0 (https://mozilla.org/MPL/2.0/). Changed by Ahab: inherited
; queries are written out in place of their `; inherits:` lines, and
; the ones apps/ui/languages/sources.json adds (`inherit`) come first.
[
  (true)
  (false)
] @constant.builtin.boolean
(null) @constant.builtin
(number) @constant.numeric

(string) @string
(escape_sequence) @constant.character.escape

(pair
  key: (_) @variable.other.member)

(comment) @comment

["," ":"] @punctuation.delimiter
[
  "["
  "]"
  "{"
  "}"
] @punctuation.bracket
