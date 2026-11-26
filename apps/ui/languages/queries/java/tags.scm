; From Helix (https://github.com/helix-editor/helix), commit 872b240251876e56787ad7ec312cefae70e3e28e,
; runtime/queries/java/tags.scm. Licensed under the Mozilla Public
; License 2.0 (https://mozilla.org/MPL/2.0/). Changed by Ahab: inherited
; queries are written out in place of their `; inherits:` lines, and
; the ones apps/ui/languages/sources.json adds (`inherit`) come first.
(class_declaration
  name: (identifier) @definition.class)

(interface_declaration
  name: (identifier) @definition.interface)

(record_declaration
  name: (identifier) @definition.class)

(enum_declaration
  name: (identifier) @definition.class)

(method_declaration
  name: (identifier) @definition.function)

(constructor_declaration
  name: (identifier) @definition.function)

(compact_constructor_declaration
  name: (identifier) @definition.function)

(field_declaration
  declarator: (variable_declarator
    name: (identifier) @definition.constant))

(enum_constant
  name: (identifier) @definition.constant)
