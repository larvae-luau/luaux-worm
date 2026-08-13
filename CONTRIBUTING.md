# How to change this worm

Read [README.md](README.md) first. It tells you what the worm does. This
document tells you how to change it.

## The loop

```sh
cargo test                 # the unit tests and the protocol tests
cargo clippy --all-targets # no warnings
cargo fmt                  # before each commit
```

For a change to the layout, build the worm and run larvae over a real file:

```sh
cargo build --release && cp target/release/luaux-worm worm.toml dist/
larvae worm run --fmt dist app.luaux
```

The command reports whether a second format changes the output. Read that line
on every run. A formatter that moves between two outputs turns each save into a
diff.

## Where each thing is

| File | What it holds |
|---|---|
| [src/main.rs](src/main.rs) | The `Handler` of the three jobs, and `serve` |
| [src/settings.rs](src/settings.rs) | `luaux.toml` under the settings of larvae |
| [src/report.rs](src/report.rs) | The messages that a person reads |
| [src/scan.rs](src/scan.rs) | Where the markup is, where the Luau is, and where each comment, hole, and node is |
| [src/statements.rs](src/statements.rs) | Where each statement is, which is where a `host` span may start and stop |
| [src/format/mod.rs](src/format/mod.rs) | The file: `host` spans, nodes, and the space between them |
| [src/format/node.rs](src/format/node.rs) | An element, a fragment, a tag, and an attribute |
| [src/format/children.rs](src/format/children.rs) | The content of an element, and where a break is safe |
| [src/format/text.rs](src/format/text.rs) | A text run, the whitespace rule, and the fill |
| [src/format/hole.rs](src/format/hole.rs) | A `{...}` hole |
| [src/format/options.rs](src/format/options.rs) | The format options that a project sets |
| [src/shadow.rs](src/shadow.rs) | The Luau view that the inherited lints of larvae read, and that the statement walk parses |
| [src/lints/mod.rs](src/lints/mod.rs) | The lint job |
| [src/lints/compiler.rs](src/lints/compiler.rs) | The findings of the luaux compiler, and where each one belongs |
| [src/lints/rules/](src/lints/rules/) | One rule of this worm per file |
| [worm.toml](worm.toml) | What the worm declares to larvae |
| [tests/protocol.rs](tests/protocol.rs) | The worm over the protocol, as larvae drives it |

The luaux crate owns the lexer, the markup parser, and the compiler. The worm
owns the layout of the markup and nothing more. If you write a rule about Luau
syntax, you are in the wrong repository.

## The rules that are easy to break

1. **Never write to stdout.** stdout is the protocol channel, and one extra byte
   corrupts a reply. Use `eprintln!` for a message.
2. **Never keep state between files.** larvae processes files in parallel, and
   work stealing decides which worker sees which file. State that crosses files
   makes the output depend on the schedule. The settings from `init` are the one
   exception, because larvae sends them before the first file.
3. **Build the document from spans, never from the strings in the syntax tree.**
   The tree decodes an escape and joins the lines of a text run while it parses,
   so it cannot give back its own input.
4. **Never format Luau.** Emit a `host` span, and let larvae format it. Two
   opinions about Luau in one file disagree with each other. The worm parses
   the shadow to find where a statement ends, and for nothing else.
5. **A `host` span holds whole statements.** larvae parses each span on its own
   and refuses one that opens a block and does not close it. The front of a
   statement is not a statement, so it crosses as `Doc::src` instead.
6. **A break sits inside the `Indent` that raises it.** `indent(concat([Hard,
   span]))` raises the span, and `concat([Hard, indent(span)])` raises
   nothing.
7. **Never change the line count in `transform`.** larvae keeps line numbers
   through the pipeline. The luaux backend emits the same number of newlines as
   the span that it replaces, so add nothing around it.
8. **Never report a lint name that `worm.toml` does not declare.** larvae counts
   such a finding as an error against the file. A protocol test reads the
   manifest and checks each name that the worm reports. A level is `allow`,
   `warn`, or `deny`, and larvae refuses the manifest for any other word.
9. **Never emit the final newline of a file.** larvae adds it.
10. **Never break a text run that holds a space.** That space is text, and a
   break deletes it. `Doc::Line` between two lines of a run is safe, because
   luaux joins the lines of a run with one space.
11. **Keep the byte length and the newlines of every region that the shadow
   replaces.** larvae reads the shadow with its own lints and reports a finding
   at the offset that it finds. A shadow that moves an offset by one byte
   reports every one of those findings in the wrong place.
12. **Answer a file that does not compile.** The editor sends the file on every
    keystroke, and a file that somebody is typing is broken most of the time.
    Report what the rules can read and mark the rest where it is. An error
    reply is for a file that gives nothing at all.

## To add a format option

A format option of a worm sits in the `[fmt]` table of `larvae.toml`, beside
`column_width`. An option holds one scalar: a boolean, an integer, a float, or
a string.

1. Declare it in `[fmt]` in `worm.toml`, with `type`, `default`, and
   `description`, and `values` when it takes a word from a list. Start the name
   with `luaux_`, because larvae refuses a name that it or an other worm
   already uses. The description becomes the hover text of the editor. A test
   reads the manifest and checks that each declared name reaches a field, so a
   declaration with no code behind it fails the suite.
2. Ask first whether larvae already has the setting. `column_width`,
   `indent_width`, and `space_inside_braces` are settings of larvae, and the
   worm follows them under the names of larvae. Declare an option only for
   something that larvae does not describe, such as the quotes of an attribute,
   which are markup and not a Luau string.
3. Add the field to `Options` in
   [src/format/options.rs](src/format/options.rs), with the default that
   Prettier or Biome gives the same rule. A `.luaux` file that no project sets
   an option for must look the same after the change.
4. Read the name in `Options::put`, and name the values that it takes in the
   error.
5. Read `layout.options` where the layout decides. Do not read an option in
   more than one place: one option, one decision.
6. Add a test for the option under `json_under`, beside the tests of that file.
7. Add the option to the table in [README.md](README.md).

An option must not change what the file means. The formatter changes the space
between the bytes of the markup, and never the bytes of a string, of a comment,
or of Luau.

## To add a lint

One rule is one file, in the same way as Biome keeps one rule per file.

1. Declare the name in `[lints]` in `worm.toml`, with a description and a
   default level. The name is bare: larvae puts it under the key of this worm,
   so `useless_fragment` reads as `luaux.useless_fragment` everywhere outside
   this repository, and no name of this worm can take the name of a builtin.
   Write the description as one line about what is wrong, because an editor
   shows it as hover text and `larvae lint --explain` prints it.
2. Write `src/lints/rules/<name>.rs`. It holds the name as a `NAME` constant,
   and a `check(src, node, out)` function that reads the syntax tree and pushes
   a finding for each problem that it sees.
3. Add the module to [src/lints/rules/mod.rs](src/lints/rules/mod.rs), and call
   it in `findings`.
4. Give a `help` line when there is a short fix to state.
5. Do not give a severity. larvae stamps the level from `[lint.rules]` over the
   default in the manifest. A worm cannot decide that a build fails.
6. Add a test for the finding, for a source that is close to it and is correct,
   and for the same markup inside a hole.
7. Add the rule to the table in [README.md](README.md).

A rule reads the syntax tree, so its span is a span of the file. A finding of
the luaux compiler is different: luaux compiles the expression inside a hole as
a source of its own, so `src/lints/compiler.rs` moves such a finding back into
the file. Do not build a rule on that path.

## Style

- Comments and documentation are in ASD-STE100 Simplified Technical English:
  active voice, simple present tense, one topic per sentence, no idioms, and one
  term for one concept.
- A comment says why, and not what. The code says what.
- One term for one concept: a *span* is a byte range, a *hole* is `{...}`, a
  *run* is a text child, a *piece* is a part of the content of an element that a
  line break must not split.
- A test name is a sentence about the behaviour, such as
  `a_text_run_keeps_the_space_that_is_text`.
- A test about layout asserts on the JSON of the document, because that JSON is
  the contract with larvae.

## Before you send a change

- `cargo test`, `cargo clippy --all-targets`, and `cargo fmt` are clean.
- A change to the layout has a test that pins the new JSON.
- A change to the manifest has a matching change in the README table.
- The output of `larvae worm run --fmt` is the same after a second run.
