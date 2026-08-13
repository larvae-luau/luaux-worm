//! The worm over the protocol that larvae speaks.
//!
//! Each message is a 4 byte little endian length, then that many bytes of JSON,
//! over stdin and stdout. These tests start the executable and speak that
//! protocol, because the unit tests prove the parts and this proves the program.

use std::io::{Read, Write};
use std::process::{Child, Command, Stdio};

use serde_json::{Value, json};

/// A `.luaux` file that holds one of every shape that the worm reads.
const SOURCE: &str = "\
local create = require(vide).create

local function App(props)
\treturn <Frame Size={props.size} Name=\"root\">
\t\t<!-- the title of the panel -->
\t\t<TextLabel Text=\"Name: {props.name}\"/>
\t\t{props.children}
\t</Frame>
end

return App
";

struct Worm {
    process: Child,
}

impl Worm {
    /// Starts the worm with the settings that larvae sends first.
    fn start() -> Self {
        Self::start_with("")
    }

    /// The same, with a `[fmt]` table of the project.
    fn start_with(fmt: &str) -> Self {
        let process = Command::new(env!("CARGO_BIN_EXE_luaux-worm"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("the worm starts");

        let mut worm = Self { process };
        let reply = worm.ask(json!({
            "op": "init",
            "config": "",
            "rules": "",
            "doc_version": 1,
            "fmt": fmt,
            "lint": "",
        }));

        assert_eq!(reply["ok"], json!(true), "{reply}");

        worm
    }

    /// Sends one request and reads one reply.
    fn ask(&mut self, request: Value) -> Value {
        let body = serde_json::to_vec(&request).expect("json");
        let input = self.process.stdin.as_mut().expect("stdin");

        input
            .write_all(&(body.len() as u32).to_le_bytes())
            .expect("a length");
        input.write_all(&body).expect("a body");
        input.flush().expect("a flush");

        let output = self.process.stdout.as_mut().expect("stdout");
        let mut length = [0u8; 4];
        output.read_exact(&mut length).expect("a length");

        let mut reply = vec![0u8; u32::from_le_bytes(length) as usize];
        output.read_exact(&mut reply).expect("a body");

        serde_json::from_slice(&reply).expect("json")
    }

    fn run(&mut self, op: &str, source: &str) -> Value {
        self.ask(json!({ "op": op, "source": source }))
    }
}

impl Drop for Worm {
    fn drop(&mut self) {
        // Larvae closes the pipe to stop a worm. The test does the same, and
        // then waits, so that no worm outlives the run.
        drop(self.process.stdin.take());
        let _ = self.process.wait();
    }
}

/// Every lint name that `worm.toml` declares.
///
/// larvae counts a finding under a name that the manifest does not declare as
/// an error against the file, so the manifest and the code must agree.
fn declared_lints() -> Vec<String> {
    let manifest: toml::Table = include_str!("../worm.toml").parse().expect("worm.toml");

    manifest["lints"]
        .as_table()
        .expect("a table of lints")
        .keys()
        .cloned()
        .collect()
}

#[test]
fn transform_gives_luau_with_the_same_line_count() {
    let reply = Worm::start().run("transform", SOURCE);

    assert_eq!(reply["ok"], json!(true), "{reply}");

    let output = reply["output"].as_str().expect("the output");

    assert_eq!(output.lines().count(), SOURCE.lines().count(), "{output}");
    assert!(!output.contains('<'), "{output}");
}

#[test]
fn format_gives_a_document_and_the_comments() {
    let reply = Worm::start().run("format", SOURCE);

    assert_eq!(reply["ok"], json!(true), "{reply}");
    assert_eq!(reply["doc"], json!(1), "{reply}");

    // The Luau around the markup goes back to larvae as a span to format.
    let document = reply["document"].to_string();
    assert!(document.contains(r#""parse":"block""#), "{document}");
    assert!(document.contains(r#""parse":"expr""#), "{document}");

    // The one comment of the file, so larvae can refuse a layout that loses it.
    let comments = reply["comments"].as_array().expect("the comments");
    assert_eq!(comments.len(), 1, "{reply}");

    let (start, end) = (
        comments[0][0].as_u64().expect("a start") as usize,
        comments[0][1].as_u64().expect("an end") as usize,
    );

    assert_eq!(&SOURCE[start..end], "<!-- the title of the panel -->");
}

#[test]
fn lint_gives_findings_under_the_names_of_the_manifest() {
    // One line for each rule, so the manifest answers for every name that the
    // worm reports.
    let source = format!(
        "{SOURCE}
local bad = <Frmae Text=\"a\" Text=\"b\">{{show and <Frame/> or nil}}</Frmae>
local more = <Frame Visible={{true}}>
\t<><TextLabel/></>
\t<TextLabel>-- a note</TextLabel>
\t<TextButton></TextButton>
</Frame>
"
    );

    let reply = Worm::start().run("lint", &source);

    assert_eq!(reply["ok"], json!(true), "{reply}");

    let findings = reply["findings"].as_array().expect("the findings");
    let declared = declared_lints();

    // Every rule of the worm answers here, and so does the compiler.
    let names: Vec<&str> = findings
        .iter()
        .map(|finding| finding["lint"].as_str().expect("a name"))
        .collect();

    for name in [
        "luaux_unresolved_name",
        "luaux_static_conditional_child",
        "luaux_duplicate_attribute",
        "luaux_self_closing_element",
        "luaux_useless_fragment",
        "luaux_explicit_true_attribute",
        "luaux_comment_as_text",
    ] {
        assert!(names.contains(&name), "{name} is missing from {names:?}");
    }

    for finding in findings {
        let name = finding["lint"].as_str().expect("a name");

        assert!(
            declared.contains(&name.to_string()),
            "{name} is not declared"
        );

        // A finding carries no severity. The host stamps the level.
        assert!(finding.get("severity").is_none(), "{finding}");

        let start = finding["span"][0].as_u64().expect("a start") as usize;
        let end = finding["span"][1].as_u64().expect("an end") as usize;

        assert!(end <= source.len() && start <= end, "{finding}");
    }
}

#[test]
fn lint_gives_larvae_the_luau_shadow_of_the_file() {
    let reply = Worm::start().run("lint", SOURCE);

    let shadow = reply["luau"].as_str().expect("a shadow");

    // The shadow is the file, byte for byte, with the markup replaced. Each
    // offset in it is an offset of the source, so larvae maps no spans.
    assert_eq!(shadow.len(), SOURCE.len(), "{shadow}");
    assert_eq!(
        shadow.match_indices('\n').count(),
        SOURCE.match_indices('\n').count()
    );
    assert!(!shadow.contains('<'), "{shadow}");
    // The file reads `create` and `props`, and the shadow reads them too.
    assert!(shadow.contains("create"), "{shadow}");
    assert!(shadow.contains("props.size"), "{shadow}");
}

#[test]
fn the_fmt_table_of_the_project_reaches_the_layout() {
    // larvae sends the resolved `[fmt]` table at init, and this option is one
    // that `worm.toml` declares.
    let fmt =
        r#"{"column_width":100,"space_inside_braces":false,"luaux_attribute_quotes":"single"}"#;
    let reply = Worm::start_with(fmt).run("format", SOURCE);

    assert_eq!(reply["ok"], json!(true), "{reply}");

    let document = reply["document"].to_string();

    assert!(document.contains(r#"{"lit":"'"}"#), "{document}");
    assert!(
        !document.contains(r#"{"lit":"{"},{"lit":" "}"#),
        "{document}"
    );
}

#[test]
fn one_bad_file_does_not_stop_the_worm() {
    let mut worm = Worm::start();
    let reply = worm.run("transform", "return <Frame>\n");

    assert_eq!(reply["ok"], json!(false), "{reply}");
    assert!(
        reply["error"]
            .as_str()
            .expect("a message")
            .contains("unclosed element"),
        "{reply}"
    );

    // The next file goes through, because larvae keeps one worm for a whole
    // run and one bad file must not stop a watch session.
    let reply = worm.run("transform", SOURCE);

    assert_eq!(reply["ok"], json!(true), "{reply}");
}

#[test]
fn a_file_with_no_markup_passes_through_every_job() {
    let mut worm = Worm::start();
    let source = "local x = 1 < 2\nreturn x\n";

    assert_eq!(worm.run("transform", source)["output"], json!(source));
    assert_eq!(worm.run("format", source)["ok"], json!(true));
    assert_eq!(worm.run("lint", source)["findings"], json!([]));
}
