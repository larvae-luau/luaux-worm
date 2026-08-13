//! The findings of a `.luaux` file.
//!
//! A finding carries no severity, by design. larvae stamps the level from
//! `[lint.rules]` over the default in `worm.toml`, applies the suppression,
//! renders the output, and decides the exit code. A worm cannot decide that a
//! build fails.
//!
//! The findings come from two places. [`compiler`] gives what the luaux
//! compiler reports while it builds the file. [`rules`] gives what this worm
//! reads from the markup itself, one rule per file.

mod compiler;
mod rules;

use larvae_worm::native::Lint;
use luaux::Config;

use crate::scan;
use crate::shadow;

/// Every name that a finding carries appears in `[lints]` in `worm.toml`.
/// larvae counts a finding under a name that the manifest does not declare as
/// an error against the file.
///
/// A file that does not compile still gets an answer. The editor sends the
/// file on every keystroke, and a file that somebody is typing is broken most
/// of the time, so this reports what it can read and marks the rest where it
/// is. An error reply is for a file that gives nothing at all.
pub fn lint(src: &str, config: &Config) -> Result<Lint, String> {
    let marks = scan::marks(src);

    // The rules read the syntax tree of the markup, and they answer for the
    // part of a broken file that still parses.
    let mut findings = rules::findings(src);

    match compiler::findings(src, config, &marks.holes) {
        Ok(more) => findings.extend(more),
        Err(refusal) => findings.push(refusal),
    }

    // A list of findings reads from the top of the file to the bottom.
    findings.sort_by_key(|finding| finding.span);

    Ok(Lint {
        findings,
        // The lints of larvae read this, and each offset in it is an offset of
        // the source. A file that does not parse has no shadow, and larvae then
        // reads the output of `transform` and maps a finding by line.
        luau: shadow::view(src, config),
        comments: marks.comments,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// luaux checks that the element factory is in scope, so every file that
    /// holds markup imports it. The default factory is a bare `create`.
    pub(super) const IMPORT: &str = "local create = require(vide).create\n";

    #[test]
    fn gathers_the_findings_of_the_compiler_and_of_the_rules() {
        let src = format!("{IMPORT}return <Frmae Text=\"a\" Text=\"b\"></Frmae>\n");
        let findings = lint(&src, &Config::default()).expect("lint").findings;
        let names: Vec<&str> = findings
            .iter()
            .map(|finding| finding.lint.as_str())
            .collect();

        assert!(names.contains(&"luaux_unresolved_name"), "{names:?}");
        assert!(names.contains(&"luaux_duplicate_attribute"), "{names:?}");
        assert!(names.contains(&"luaux_self_closing_element"), "{names:?}");
    }

    #[test]
    fn reads_the_file_from_the_top_to_the_bottom() {
        let src = format!("{IMPORT}return <Frame>\n\t<Frmae/>\n\t<Frmea/>\n</Frame>\n");
        let findings = lint(&src, &Config::default()).expect("lint").findings;

        assert!(findings.len() >= 2, "{findings:?}");
        assert!(findings.windows(2).all(|pair| pair[0].span <= pair[1].span));
    }

    fn names(src: &str) -> Vec<String> {
        lint(src, &Config::default())
            .expect("an answer")
            .findings
            .iter()
            .map(|finding| finding.lint.clone())
            .collect()
    }

    #[test]
    fn a_file_that_does_not_compile_still_gives_the_findings_of_the_rules() {
        // The factory is not in scope, so the compiler stops. The markup parses
        // all the same, and the rules read it.
        let names = names("return <Frame Text=\"a\" Text=\"b\"/>\n");

        assert!(
            names.contains(&String::from("luaux_duplicate_attribute")),
            "{names:?}"
        );
        assert!(
            names.contains(&String::from("luaux_compile_error")),
            "{names:?}"
        );
    }

    #[test]
    fn a_file_that_does_not_parse_gives_the_place_of_the_problem() {
        // A buffer in an editor looks like this between two keystrokes. The
        // markup parser has no recovery, so the mark is all there is.
        let result = lint("return <Frame>\n", &Config::default()).expect("an answer");

        assert_eq!(result.findings.len(), 1, "{result:?}");
        assert_eq!(result.findings[0].lint, "luaux_compile_error");
        assert!(result.findings[0].message.contains("unclosed element"));

        // The mark holds bytes, so an editor has something to underline.
        let (start, end) = result.findings[0].span;
        assert!(end > start, "{result:?}");
    }

    #[test]
    fn gives_larvae_the_comments_that_hide_a_finding() {
        let allow = "-- larvae: allow(luaux_static_conditional_child)\n";
        let src = format!("{allow}{IMPORT}return <Frame/>\n");
        let result = lint(&src, &Config::default()).expect("lint");

        assert_eq!(result.comments, [(0, allow.len() as u32 - 1)]);
    }
}
