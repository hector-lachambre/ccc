# ccc, a compiler for Conventional Commits

`ccc` reads a commit message and treats it the way a compiler treats a source file: a lexer
produces tokens, a parser builds a syntax tree, a semantic pass checks the tree against the
[Conventional Commits 1.0.0](https://www.conventionalcommits.org/en/v1.0.0/) specification
and the rules you configure, and an emitter writes out what you asked for. Every problem is
reported with its exact position, rustc-style.

```
$ ccc -m 'feet(ui button): Add thing.'
error[E102]: unknown type `feet`
 --> <message>:1:1
  |
1 | feet(ui button): Add thing.
  | ^^^^ not an allowed type
  |
  = help: did you mean `feat`?

error[E105]: the scope must not contain whitespace
 --> <message>:1:6
  |
1 | feet(ui button): Add thing.
  |      ^^^^^^^^^ scope
  |
  = help: write `(ui-button)`

warning[W101]: the description should start in lowercase
 --> <message>:1:18
  |
1 | feet(ui button): Add thing.
  |                  ^^^ capitalised
  |
  = help: write `add`

error[E106]: the description must not end with a period
 --> <message>:1:27
  |
1 | feet(ui button): Add thing.
  |                           ^ remove this period

error: the message was rejected due to 3 errors; 1 warning emitted
```

## Install

```
cargo install --path .
```

The crate is `conventional-commit-compiler`; the library is `ccc`, the binary is `ccc`.

## Usage

```
ccc [OPTIONS] [FILE]
```

The message comes from `FILE`, from standard input when `FILE` is omitted or `-`, or from
`-m TEXT`. The exit status is 0 when the message is accepted, 1 when it is rejected, 2 on a
usage or I/O error. Nothing is written on standard output unless `--emit` asks for it.

| Option | Effect |
| --- | --- |
| `--emit ast` | The syntax tree as JSON, when the message is accepted |
| `--emit report` | Acceptance, bump, tree and diagnostics as JSON, always |
| `--emit bump` | The version bump the message calls for: `major`, `minor`, `patch` or `none` |
| `--emit canonical` | The message rewritten in canonical form, when it is accepted |
| `-t, --types LIST` | Allowed types, comma-separated. Default: the Angular set. `any` lifts the restriction |
| `-s, --scopes LIST` | Allowed scopes. Any scope is accepted when omitted |
| `--require-scope` | Reject messages without a scope |
| `--max-header N` | Header length limit in characters, default 100, 0 disables |
| `--max-body-line N` | Body line length limit, default 100, 0 disables |
| `--allow-capitalized` | Accept a description that starts with a capital letter |
| `--keep-comments` | Keep lines starting with `#` instead of dropping them like git does |
| `-D, --deny-warnings` | Treat warnings as errors |
| `-q, --quiet` | Write nothing on standard error |
| `--color WHEN` | `auto`, `always` or `never`; `NO_COLOR` is honoured |

### As a git hook

```sh
#!/bin/sh
# .git/hooks/commit-msg
exec ccc "$1"
```

Git comment lines are dropped before parsing, so the hook sees exactly what git will record.

### Bump of a commit, for a release script

```sh
git log -1 --format=%B HEAD | ccc --emit bump -q
```

`ccc` compiles one message at a time; a script that walks a range keeps the highest bump.

## What is checked

The parser enforces the grammar. The semantic pass enforces the rest of the specification and
the configured rules. Codes are stable.

| Code | Severity | Condition |
| --- | --- | --- |
| E001 | error | the message does not start with a type |
| E002 | error | the scope is empty, `feat(): x` |
| E003 | error | the scope is not closed |
| E004 | error | a space precedes the `:` |
| E005 | error | no `:` follows the type |
| E006 | error | no space follows the `:` |
| E007 | error | the description is empty |
| E008 | error | the body is not separated from the header by a blank line |
| E101 | error | the type is not lowercase |
| E102 | error | the type is not in the allowed list |
| E103 | error | a scope is required and missing |
| E104 | error | the scope is not in the allowed list |
| E105 | error | the scope contains whitespace |
| E106 | error | the description ends with a period |
| E107 | error | a `BREAKING CHANGE:` line sits in the body instead of the footers |
| E108 | error | the breaking change token is not written `BREAKING CHANGE` or `BREAKING-CHANGE` |
| E109 | error | a footer has no value |
| W101 | warning | the description starts with a capitalised word |
| W102 | warning | the header is longer than the limit |
| W103 | warning | a body line is longer than the limit |
| W104 | warning | `BREAKING CHANGE` uses the ` #` separator instead of `: ` |

The parser recovers from every error but E001 and E005, so one run reports everything wrong
with a message rather than the first problem only.

## How the message is read

The header is `type(scope)!: description`. What follows is split into paragraphs on blank
lines. The footers are the longest run of trailing paragraphs that each open with a footer
line, `token: value`, `token #value` or `BREAKING CHANGE: value`; every paragraph before them
is the body. Inside a footer paragraph, a line that does not open a footer continues the
previous value, as the specification allows.

A footer token is a word of letters, digits, `-` and `_` that starts with a letter or digit,
or `BREAKING CHANGE`. A body paragraph that happens to start with a footer-looking line but
is followed by prose stays body text, which keeps "Before: ..." paragraphs where they belong.
The one case this would silently mis-read is a breaking change written before a prose
paragraph, so that case is an error (E107).

## Library

```rust
use ccc::{compile, Options};
use ccc::sema::Bump;

let compilation = compile("feat(parser): accept multi-line footers", &Options::default());
assert!(compilation.is_ok());
assert_eq!(compilation.bump, Bump::Minor);
```

`Options` carries the `sema::Rules` and whether to drop git comment lines. The result holds
the `ast::Commit`, with a byte span on every node, and the diagnostics, which
`diagnostic::Renderer` prints as text and `emit::json` serialises. `emit::canonical` rewrites
an accepted message in canonical form.

Modules follow the pipeline: `lexer`, `token`, `parser`, `ast`, `sema`, `emit`, with `span`
and `diagnostic` shared by all of them.

## License

MIT, see [LICENSE](LICENSE).
