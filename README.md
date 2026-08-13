# luaux-worm

A larvae worm that gives `.luaux` files a compiler, a formatter, and a linter.

[luaux][luaux] is JSX-style markup for Luau. [larvae][larvae] is a toolchain for
Luau. This worm joins the two: larvae claims each `.luaux` file, and this worm
answers for it. The luaux crate does the compiling. The worm adds the two jobs
that only larvae can drive, which are the formatter and the linter.

The worm is the `native` form: an ordinary executable that larvae starts one
time and keeps alive. larvae and the worm speak over stdin and stdout.

[luaux]: https://github.com/luau-xml/luaux
[larvae]: https://github.com/larvae-luau/larvae

## What it does

| Job | larvae command | The worm returns |
|---|---|---|
| Compile | `larvae process`, `larvae check` | Luau source text |
| Format | `larvae fmt` | A layout document, not text |
| Lint | `larvae lint` | Findings without a severity |

The compile keeps the line count of the file. Line N of the output is line N of
the source, so a stack trace in Studio points at the line that the author wrote.

The formatter formats the markup only. Each run of ordinary Luau goes back to
larvae as a span, and larvae formats it with the width and the indentation of
the project. Thus a `.luaux` file breaks a line in the same way as a `.luau`
file, and this worm holds no printer of its own.

The linter reports a finding with no severity. larvae stamps the level, applies
`-- larvae: allow(...)`, and decides the exit code.

## What you need

- Rust 1.85 or later, for the 2024 edition.
- A larvae that names the lints of a worm under the key of the worm. Every lint
  here is a bare name, so a larvae that reads `[lint.rules.luaux]` as a level
  and not as a table refuses the project. `larvae 0.1.2-beta` is such a larvae,
  and the release after it is the first that reads the table.
- Linux, macOS, or Windows. `larvae-worm 0.1.2-beta` gates the node API of the
  wasm form behind `target_arch = "wasm32"`, so a native worm links on each of
  them. An earlier crate gives `link.exe` nine unresolved symbols.

## Build and install

```sh
cargo build --release
mkdir -p dist
cp target/release/luaux-worm worm.toml dist/
```

larvae reads the manifest beside the executable, so both files go in one
directory. Point the project at that directory:

```toml
# larvae.toml
[worms.luaux]
path = "path/to/luaux-worm/dist"
```

A release works as well, with a larvae that looks for
`<name>-worm-<arch>-<os>.zip` and sets the executable bit after it unpacks one.
The release job of this repository writes exactly those names, so a project
takes the worm by version and names no asset.

Check what the manifest declares, and run one file through the worm:

```sh
larvae worm info dist
larvae worm run dist app.luaux          # compile
larvae worm run --fmt dist app.luaux    # format, with a report about idempotence
larvae worm run --lint dist app.luaux   # lint
```

## Settings

`luaux.toml` stays the file of the luaux project. The worm reads it from the
directory that larvae runs in, and writes nothing to it, so the luaux command
and this worm compile a file the same way.

The settings of larvae belong in `larvae.toml`, and each kind has one table.
The lints of this worm sit beside the builtin lints, and its format options sit
beside `column_width`, so a reader learns one place for each kind and not one
place per worm:

```toml
[worms.luaux]
path = "path/to/luaux-worm/dist"

# The format options, beside the format options of larvae.
[fmt]
column_width           = 100
indent_type            = "tabs"
luaux_attribute_quotes = "double"
luaux_text_wrap        = "fill"

# The level of each lint of this worm, under the key of this worm.
[lint.rules.luaux]
static_conditional_child = "warn"

# The builtin lints of larvae, in `.luaux` files too.
[lint.rules]
shadowing = "deny"
```

The width and the indentation of the output are settings of larvae, because
larvae owns the printer. The level of a lint is a setting of larvae as well,
because larvae owns every level: the worm reports each finding, and larvae
decides what the finding means.

`[worms.luaux.config]` holds the settings of this worm alone, and there is one:

```toml
[worms.luaux.config]
# Where the luaux.toml of the project is, from the directory larvae runs in.
luaux_toml = "src/luaux.toml"
```

Everything that the luaux compiler means stays in `luaux.toml`: the factory,
the aliases, and the build paths. The worm reads that file the way the luaux
command reads it, and moves nothing out of it. luaux is a tool of its own, and
a user with no larvae project keeps a working luaux.

## The format options

larvae owns the width, the indentation, and the printer. What is left is the
markup itself, and each decision about it is one option below. `worm.toml`
declares each one, so `larvae init` writes it with its default and its
description, and the editor completes it.

The worm follows the options of larvae as well, for what it lays out itself.
`indent_type` and `indent_width` say how deep a node sits, and
`space_inside_braces` gives `{ expr }` or `{expr}`. A project states each of
those one time, for its `.luau` files and its `.luaux` files together.

| Name | Default | What it decides | Prettier or Biome |
|---|---|---|---|
| `luaux_attribute_quotes` | `"double"` | The quotes of the string of an attribute: `"double"`, `"single"`, or `"preserve"`. The text between the quotes never changes, so a string that holds the other quote keeps the quotes that it has. This is not `quote_style`, which governs Luau strings: an attribute value is markup that only looks like Luau. | `jsxQuoteStyle` |
| `luaux_bracket_same_line` | `false` | Where the `>` of a tag goes when the tag breaks. `false` puts it on a line of its own, under the name. `true` puts it after the last attribute. | `bracketSameLine` |
| `luaux_attribute_per_line` | `false` | `false` keeps the attributes on one line while they fit it. `true` gives one attribute per line at all times. | `singleAttributePerLine` |
| `luaux_self_closing_space` | `true` | The space in `<Frame />`. `false` writes `<Frame/>`. | `useSelfClosingElements` |
| `luaux_text_wrap` | `"fill"` | `"fill"` fills each line with as many words as it holds. `"preserve"` breaks the text where the author broke it. | `proseWrap` |
| `luaux_blank_lines` | `true` | Whether a blank line between two children stays. | — |

## The lints

Each name below is bare, and larvae puts it under the key of this worm. So a
lint of this worm reads as `luaux.<name>` outside this repository, a project
writes every one of them together, and a name here can never take the name of a
builtin:

```toml
[lint.rules.luaux]
useless_fragment = "allow"
compile_error    = "deny"
```

A level is `allow`, `warn`, or `deny`, and `deny` is the level that fails the
run. `larvae lint --explain luaux.<name>` reads the description of each one,
and `-- larvae: allow(luaux.<name>)` hides one finding.

The luaux compiler reports the first four while it builds the file:

| Name | Default | What it reports |
|---|---|---|
| `compile_error` | `deny` | A problem that stops the compiler, such as an element that no tag closes. It is a finding, and not a message about the whole file, so an editor marks the bytes that are wrong. |
| `static_conditional_child` | `warn` | Markup in a child expression that no function encloses. The code builds the child one time, so a condition around it looks live and is not. Wrap it in a function, as `{function() return ... end}`, and Vide tracks it. |
| `unresolved_name` | `deny` | An element that is neither a Roblox class nor a component in scope, or a property that the class does not have. |
| `compile_warning` | `warn` | A warning of the luaux compiler that this worm has no more exact name for. |

The worm reads the rest from the markup itself. Each one follows a rule of
[Biome][biome] for JSX, and says the same thing for markup that Vide builds:

| Name | Default | What it reports | Biome |
|---|---|---|---|
| `duplicate_attribute` | `deny` | The same attribute name two times on one element. The last value wins, and the other line does nothing. | `noDuplicateJsxProps` |
| `self_closing_element` | `warn` | `<Frame></Frame>`, which is `<Frame/>` with more to read. | `useSelfClosingElements` |
| `useless_fragment` | `warn` | A fragment inside another node. A fragment is a plain table, and Vide reads a table in a child slot as a list of children, so the level goes away again. | `noUselessFragments` |
| `explicit_true_attribute` | `warn` | `Visible={true}`, where `Visible` means the same. | `noImplicitBoolean` |
| `comment_as_text` | `warn` | A text child that starts with `--` or `//`. Between two tags that is text, and the player reads it on the screen. | `noCommentText` |

[biome]: https://biomejs.dev

### The lints of larvae, in a `.luaux` file too

`inherit_lints = true` in the manifest asks larvae to run its own lints on a
`.luaux` file as well, so `unused_variable`, `shadowing`, and the rest report in
a claimed file the same way as in a `.luau` file. The user levels them in
`[lint.rules]`, and turns them off with `[worms.luaux] inherit_lints = false`.

Those lints read Luau, so the worm hands larvae a shadow of the file: the same
bytes, with each markup region replaced by a table of the same byte length. The
newlines stay where they are, so a finding lands on the right line and column,
and each part of the markup that reads a value of the file keeps its place
inside that table:

```text
local size = UDim2.fromScale(1, 1)     local size = UDim2.fromScale(1, 1)
return <Frame Size={size}>          -> return {          size,
    <Row Name="a"/>                            {Row         },
</Frame>                                }
```

`Row` and `size` are still read, so `unused_variable` stays quiet about them.
`Frame` is a Roblox class and not a name of the file, so the shadow drops it and
`undefined_variable` stays quiet as well.

## How the formatter reads a file

The defaults of the rules above follow Prettier and Biome, because a reader of
`.luaux` reads that shape in other languages as well. A tag is flat while it
fits the line. It breaks with one attribute per line and the closing bracket at
the left. Each child that is not text takes one line. Text fills the line, and
reads as a paragraph:

```luau
return <TextLabel
	TextSize={18}
	Size={UDim2.fromOffset(240, 80)}
>
	Welcome to the shop. Buy a hat here, and wear
	it in every place that you visit.
</TextLabel>
```

The rules under that layout:

- **Markup only.** The worm lays out markup, and larvae lays out Luau. A run of
  whole statements that holds no markup goes to larvae as a `host` span, and
  each `{expr}` hole goes as a `host_expr` span. larvae parses every span on its
  own and refuses one that opens a block and does not close it, so a span is
  never a part of a statement: the statement that holds the markup crosses byte
  for byte, and the worm changes nothing in it but the markup.
- **An attribute string is not a Luau string.** It crosses as a span, and never
  as `host_expr`, so `quote_style` does not requote what the author wrote.
- **Spans, never strings.** The luaux syntax tree decodes an escape and joins
  the lines of a text run while it parses, so the tree cannot give back the text
  that the author wrote. Every part of the output comes from a span of the
  source.
- **The whitespace rule.** luaux drops the whitespace next to a newline, because
  that whitespace is indentation. It keeps the other whitespace, because that
  whitespace is text, and it joins the lines that are left with one space. So a
  newline and one space between two words are the same thing, and the text
  wraps freely at either one. Two spaces are not the same thing, and neither is
  a space at the edge of a text run: the worm never breaks there, because
  `Name: {x}` over two lines becomes `Name:{x}` and loses the space.
- **Comments.** Every comment crosses as a span, and the worm names every span
  in the reply. larvae refuses a layout that lost a comment and leaves the file
  as it is, because to delete a comment is worse than to refuse to format a
  file.
- **Blank lines.** A blank line between two children stays, because the author
  wrote it to separate one idea from the next.

## What it does not do yet

- **A hole that mixes Luau and markup keeps its bytes.** `{cond and <A/> or nil}`
  goes to larvae as source text, and nothing inside it is reformatted. A part of
  a mixed expression is not an expression on its own, so larvae cannot parse it,
  and this worm does not format Luau. A hole that holds Luau only, or one node
  only, is formatted. A hole that holds a comment keeps its bytes as well,
  because the Luau emitter of larvae drops a comment beside an expression, and
  larvae then refuses the whole file to save it.
- **A statement that holds markup keeps its Luau.** The Luau of that statement
  crosses byte for byte, because a part of a statement is not a statement and
  larvae parses no such thing. Every other statement of the file follows the
  style of the project. So a file that indents with spaces in a project that
  asks for tabs keeps its spaces on that one statement, while the node inside
  it takes the tabs of the project.
- **A finding from inside a hole is placed by its message.** luaux compiles the
  expression in a hole as a source of its own, so it counts such a finding from
  the start of that expression. The worm knows where each hole starts and puts
  the finding back in the file, at the first hole where the source agrees with
  the message.

## Tests

```sh
cargo test
```

The unit tests build a layout document and assert on its JSON, because that JSON
is the contract with larvae. The tests in `tests/protocol.rs` start the
executable and speak the length prefixed protocol, one message at a time. The
tests of the shadow parse it with `full_moon`, because a shadow that is not
Luau gives larvae nothing to lint.

Against a real larvae, with the worm in `dist`:

```sh
larvae worm run --fmt dist app.luaux    # reads `idempotent` on every run
larvae worm run --lint dist app.luaux
```

See [CONTRIBUTING.md](CONTRIBUTING.md) to change the worm.

## Licence

MIT.
