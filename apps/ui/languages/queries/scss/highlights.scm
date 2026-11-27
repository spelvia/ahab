; From Helix (https://github.com/helix-editor/helix), commit 872b240251876e56787ad7ec312cefae70e3e28e,
; runtime/queries/scss/highlights.scm. Licensed under the Mozilla Public
; License 2.0 (https://mozilla.org/MPL/2.0/). Changed by Ahab: inherited
; queries are written out in place of their `; inherits:` lines, and
; the ones apps/ui/languages/sources.json adds (`inherit`) come first.
[(comment) (single_line_comment)] @comment

[
 "~"
 ">"
 "+"
 "-"
 "*"
 "/"
 "="
 "^="
 "|="
 "~="
 "$="
 "*="
] @operator

[
  "in"
  "and"
  "or"
  "not"
  "only"
] @operator.control

[
  "@apply"
  "@at-root"
  "@charset"
  "@debug"
  "@error"
  "@extend"
  "@keyframes"
  "@media"
  "@mixin"
  "@supports"
  "@warn"
] @constant.builtin

[
  "@import"
  "@include"
  "@forward"
  "@use"
] @keyword.control.import

[
  "@if"
  "@else"
] @keyword.control.conditional

[
  "@each"
  "@for"
  "@while"
] @keyword.control.repeat

"@return" @keyword.control.return

"@function" @function.method
"@namespace" @namespace

(property_name) @variable.other.member

((property_name) @variable
 (#match? @variable "^--"))
((plain_value) @variable
 (#match? @variable "^--"))

(tag_name) @tag
(universal_selector) @tag
(attribute_selector (plain_value) @string)
(nesting_selector) @variable.other.member
(pseudo_element_selector) @attribute
(pseudo_class_selector) @attribute

(identifier) @variable
(class_name) @label
(id_name) @label
(namespace_name) @namespace

(feature_name) @variable.other.member
(variable) @variable
(variable_name) @variable.other.member
(variable_value) @variable.other.member
(argument_name) @variable.parameter
(selectors) @variable.other.member

(attribute_name) @attribute

(function_name) @function

(to) @keyword
(from) @keyword
(important) @keyword

(string_value) @string
(color_value) @string.special

(integer_value) @constant.numeric.integer
(float_value) @constant.numeric.float
(unit) @type

"#" @punctuation.delimiter
"," @punctuation.delimiter
