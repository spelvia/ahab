; From Helix (https://github.com/helix-editor/helix), commit 872b240251876e56787ad7ec312cefae70e3e28e,
; runtime/queries/yaml/highlights.scm. Licensed under the Mozilla Public
; License 2.0 (https://mozilla.org/MPL/2.0/). Changed by Ahab: inherited
; queries are written out in place of their `; inherits:` lines, and
; the ones apps/ui/languages/sources.json adds (`inherit`) come first.
(boolean_scalar) @constant.builtin.boolean
(null_scalar) @constant.builtin
(double_quote_scalar) @string
(single_quote_scalar) @string
(block_scalar) @string
(string_scalar) @string
(escape_sequence) @constant.character.escape
(integer_scalar) @constant.numeric.integer
(float_scalar) @constant.numeric.float
(comment) @comment
(anchor_name) @type
(alias_name) @type
(tag) @type
(yaml_directive) @keyword

(block_mapping_pair
  key: (flow_node [(double_quote_scalar) (single_quote_scalar)] @variable.other.member))
(block_mapping_pair
  key: (flow_node (plain_scalar (string_scalar) @variable.other.member)))

(flow_mapping
  (_ key: (flow_node [(double_quote_scalar) (single_quote_scalar)] @variable.other.member)))
(flow_mapping
  (_ key: (flow_node (plain_scalar (string_scalar) @variable.other.member))))

[
","
"-"
":"
">"
"?"
"|"
] @punctuation.delimiter

[
"["
"]"
"{"
"}"
] @punctuation.bracket

["*" "&" "---" "..."] @punctuation.special
