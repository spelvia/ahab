; From Helix (https://github.com/helix-editor/helix), commit 872b240251876e56787ad7ec312cefae70e3e28e,
; runtime/queries/kotlin/folds.scm. Licensed under the Mozilla Public
; License 2.0 (https://mozilla.org/MPL/2.0/). Changed by Ahab: inherited
; queries are written out in place of their `; inherits:` lines, and
; the ones apps/ui/languages/sources.json adds (`inherit`) come first.
[
	(import_list)

	(when_expression)
	(control_structure_body)

	(lambda_literal)
	(function_body)
	(primary_constructor)
	(secondary_constructor)
	(anonymous_initializer)

	(class_body)
	(enum_class_body)

	(interpolated_expression)
] @fold
