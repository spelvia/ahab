; From Helix (https://github.com/helix-editor/helix), commit 872b240251876e56787ad7ec312cefae70e3e28e,
; runtime/queries/cpp/tags.scm. Licensed under the Mozilla Public
; License 2.0 (https://mozilla.org/MPL/2.0/). Changed by Ahab: inherited
; queries are written out in place of their `; inherits:` lines, and
; the ones apps/ui/languages/sources.json adds (`inherit`) come first.
(function_declarator
  declarator: [(identifier) (field_identifier)] @definition.function)

(preproc_function_def name: (identifier) @definition.function)

(preproc_def name: (identifier) @definition.constant)

(type_definition
  declarator: (type_identifier) @definition.type)

(struct_specifier
  name: (type_identifier) @definition.struct)

(enum_specifier
  name: (type_identifier) @definition.enum)

(union_specifier
  name: (type_identifier) @definition.struct)


(function_declarator
  declarator: (qualified_identifier name: (identifier) @definition.function))

(class_specifier
  name: (type_identifier) @definition.class
  body: (field_declaration_list))

(namespace_definition
  name: (namespace_identifier) @definition.module)

(concept_definition
  name: (identifier) @definition.interface)

(alias_declaration
  name: (type_identifier) @definition.type)
