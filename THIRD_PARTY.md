# Third-party notices

What Ahab ships from others beyond its Rust and npm dependencies (whose
licences `cargo deny` and the lockfiles cover).

## Languages

The bundled languages are rebuilt from the sources pinned in
`apps/ui/languages/sources.json` with `make grammars-update`.

<!-- languages:start (written by `make grammars-update`) -->

### Tree-sitter grammars

Compiled to WebAssembly into `apps/ui/languages/grammars/` and served by Ahab.
Their licence texts are in `apps/ui/public/licenses/languages.txt`, which
Ahab serves at `/licenses/languages.txt`.

| Grammar         | Repository                                                                                                | Revision       | Licence |
| --------------- | --------------------------------------------------------------------------------------------------------- | -------------- | ------- |
| bash            | [tree-sitter/tree-sitter-bash](https://github.com/tree-sitter/tree-sitter-bash)                           | `a06c2e4415e9` | MIT     |
| c               | [tree-sitter/tree-sitter-c](https://github.com/tree-sitter/tree-sitter-c)                                 | `b780e47fc780` | MIT     |
| c-sharp         | [tree-sitter/tree-sitter-c-sharp](https://github.com/tree-sitter/tree-sitter-c-sharp)                     | `cac6d5fb595f` | MIT     |
| cpp             | [tree-sitter/tree-sitter-cpp](https://github.com/tree-sitter/tree-sitter-cpp)                             | `8b5b49eb196b` | MIT     |
| css             | [tree-sitter/tree-sitter-css](https://github.com/tree-sitter/tree-sitter-css)                             | `dda5cfc5722c` | MIT     |
| dockerfile      | [camdencheek/tree-sitter-dockerfile](https://github.com/camdencheek/tree-sitter-dockerfile)               | `971acdd90856` | MIT     |
| go              | [tree-sitter/tree-sitter-go](https://github.com/tree-sitter/tree-sitter-go)                               | `2346a3ab1bb3` | MIT     |
| html            | [tree-sitter/tree-sitter-html](https://github.com/tree-sitter/tree-sitter-html)                           | `73a3947324f6` | MIT     |
| java            | [tree-sitter/tree-sitter-java](https://github.com/tree-sitter/tree-sitter-java)                           | `e10607b45ff7` | MIT     |
| javascript      | [tree-sitter/tree-sitter-javascript](https://github.com/tree-sitter/tree-sitter-javascript)               | `58404d8cf191` | MIT     |
| json            | [tree-sitter/tree-sitter-json](https://github.com/tree-sitter/tree-sitter-json)                           | `001c28d7a298` | MIT     |
| kotlin          | [fwcd/tree-sitter-kotlin](https://github.com/fwcd/tree-sitter-kotlin)                                     | `f66d2908542e` | MIT     |
| lua             | [tree-sitter-grammars/tree-sitter-lua](https://github.com/tree-sitter-grammars/tree-sitter-lua)           | `10fe0054734e` | MIT     |
| make            | [alemuller/tree-sitter-make](https://github.com/alemuller/tree-sitter-make)                               | `a4b9187417d6` | MIT     |
| markdown        | [tree-sitter-grammars/tree-sitter-markdown](https://github.com/tree-sitter-grammars/tree-sitter-markdown) | `f969cd3ae3f9` | MIT     |
| markdown_inline | [tree-sitter-grammars/tree-sitter-markdown](https://github.com/tree-sitter-grammars/tree-sitter-markdown) | `f969cd3ae3f9` | MIT     |
| nix             | [numtide/tree-sitter-nix](https://github.com/numtide/tree-sitter-nix)                                     | `70f34e95e30b` | MIT     |
| php             | [tree-sitter/tree-sitter-php](https://github.com/tree-sitter/tree-sitter-php)                             | `3f2465c217d0` | MIT     |
| python          | [tree-sitter/tree-sitter-python](https://github.com/tree-sitter/tree-sitter-python)                       | `293fdc02038e` | MIT     |
| ruby            | [tree-sitter/tree-sitter-ruby](https://github.com/tree-sitter/tree-sitter-ruby)                           | `ad907a69da0c` | MIT     |
| rust            | [tree-sitter/tree-sitter-rust](https://github.com/tree-sitter/tree-sitter-rust)                           | `77a3747266f4` | MIT     |
| scss            | [serenadeai/tree-sitter-scss](https://github.com/serenadeai/tree-sitter-scss)                             | `c478c6868648` | MIT     |
| sql             | [DerekStride/tree-sitter-sql](https://github.com/DerekStride/tree-sitter-sql)                             | `851e9cb257ba` | MIT     |
| swift           | [alex-pinkus/tree-sitter-swift](https://github.com/alex-pinkus/tree-sitter-swift)                         | `7b7909f2f6b9` | MIT     |
| toml            | [tree-sitter-grammars/tree-sitter-toml](https://github.com/tree-sitter-grammars/tree-sitter-toml)         | `64b56832c2cf` | MIT     |
| tsx             | [tree-sitter/tree-sitter-typescript](https://github.com/tree-sitter/tree-sitter-typescript)               | `75b3874edb2d` | MIT     |
| typescript      | [tree-sitter/tree-sitter-typescript](https://github.com/tree-sitter/tree-sitter-typescript)               | `75b3874edb2d` | MIT     |
| yaml            | [tree-sitter-grammars/tree-sitter-yaml](https://github.com/tree-sitter-grammars/tree-sitter-yaml)         | `7708026449be` | MIT     |

### Highlighting queries

From [Helix](https://github.com/helix-editor/helix) at `872b24025187`,
under the [Mozilla Public License 2.0](https://mozilla.org/MPL/2.0/), in
`apps/ui/languages/queries/`. Ahab writes inherited queries out in place of
Helix's `; inherits:` lines; each file says where it comes from.

<!-- languages:end -->

## Language tools

The tool records in `tools/registry/` are Ahab's own; the tools they
install come from their own sources, under the licences each record
names. Some records follow others' work:

- **Root markers and run commands** follow
  [Helix](https://github.com/helix-editor/helix)'s `languages.toml`, under
  the [Mozilla Public License 2.0](https://mozilla.org/MPL/2.0/).
- **The ESLint server's settings** in
  `tools/registry/vscode-eslint-language-server.json` are adapted from
  [nvim-lspconfig](https://github.com/neovim/nvim-lspconfig)'s `eslint`
  configuration, under the
  [Apache License 2.0](https://www.apache.org/licenses/LICENSE-2.0).
