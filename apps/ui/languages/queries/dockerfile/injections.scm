; From Helix (https://github.com/helix-editor/helix), commit 872b240251876e56787ad7ec312cefae70e3e28e,
; runtime/queries/dockerfile/injections.scm. Licensed under the Mozilla Public
; License 2.0 (https://mozilla.org/MPL/2.0/). Changed by Ahab: inherited
; queries are written out in place of their `; inherits:` lines, and
; the ones apps/ui/languages/sources.json adds (`inherit`) come first.
((comment) @injection.content
 (#set! injection.language "comment"))

((shell_command (shell_fragment) @injection.content)
 (#set! injection.language "bash")
 (#set! injection.combined))

((run_instruction
 (heredoc_block (heredoc_line) @injection.content . "\n" @injection.content))
 (#set! injection.language "bash")
 (#set! injection.combined))

((copy_instruction
 (path (heredoc_marker)) . (path) @injection.filename
 (heredoc_block (heredoc_line) @injection.content . "\n" @injection.content))
 (#set! injection.combined))
