; From Helix (https://github.com/helix-editor/helix), commit 872b240251876e56787ad7ec312cefae70e3e28e,
; runtime/queries/kotlin/tags.scm. Licensed under the Mozilla Public
; License 2.0 (https://mozilla.org/MPL/2.0/). Changed by Ahab: inherited
; queries are written out in place of their `; inherits:` lines, and
; the ones apps/ui/languages/sources.json adds (`inherit`) come first.
(class_declaration
  (type_identifier) @definition.class)

(object_declaration
  "object" (type_identifier) @definition.class)

(function_declaration
  (simple_identifier) @definition.function)

(property_declaration
  (variable_declaration
    (simple_identifier) @definition.constant))
