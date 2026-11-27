; From Helix (https://github.com/helix-editor/helix), commit 872b240251876e56787ad7ec312cefae70e3e28e,
; runtime/queries/markdown.inline/highlights.scm. Licensed under the Mozilla Public
; License 2.0 (https://mozilla.org/MPL/2.0/). Changed by Ahab: inherited
; queries are written out in place of their `; inherits:` lines, and
; the ones apps/ui/languages/sources.json adds (`inherit`) come first.
;; From nvim-treesitter/nvim-treesitter
[
  (code_span)
  (link_title)
] @markup.raw.inline

[
  (emphasis_delimiter)
  (code_span_delimiter)
] @punctuation.bracket

(emphasis) @markup.italic

(strong_emphasis) @markup.bold

(strikethrough) @markup.strikethrough

[
  (link_destination)
  (uri_autolink)
] @markup.link.url

[
  (link_text)
  (image_description)
] @markup.link.text

(link_label) @markup.link.label

[
  (backslash_escape)
  (hard_line_break)
] @constant.character.escape

(image ["[" "]" "(" ")"] @punctuation.bracket)
(image "!" @punctuation.special)
(inline_link ["[" "]" "(" ")"] @punctuation.bracket)
(shortcut_link ["[" "]"] @punctuation.bracket)
