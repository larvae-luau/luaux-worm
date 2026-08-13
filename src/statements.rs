//! Where each statement of the file starts and ends.
//!
//! larvae parses every `host` span on its own, and it refuses a span that opens
//! a block and does not close it. `Doc::host` over `local function App()` alone
//! reports `expected end, found end of file`. So a span must hold whole
//! statements, and the text between two markup regions is not one: the Luau
//! before a region is the front of a statement, and the Luau after it is the
//! back.
//!
//! The shadow answers where the statements are. It is Luau, it parses, and each
//! offset in it is the same offset in the source, so a parse of the shadow
//! gives the ranges of the source. See [`crate::shadow`].
//!
//! The worm reads the shadow to find a boundary, and never to format. larvae
//! formats every span that this module names, and the bytes of every other part
//! of the file cross as the author wrote them.

use full_moon::node::Node;

/// A run of whole statements that holds no markup.
///
/// larvae formats a run as one block, so a comment between two statements of
/// the run stays with the statement that it belongs to.
#[derive(Debug, PartialEq, Eq)]
pub struct Run {
    pub start: usize,
    pub end: usize,
}

/// The runs of the file that larvae can format, in source order.
///
/// `markup` holds the range of each markup region. A statement that holds one
/// is not a run: the worm lays that statement out itself, byte for byte, with
/// the markup inside it.
///
/// A shadow that does not parse gives no run. The file then crosses as it is,
/// which is the right answer for a file that somebody is still typing.
pub fn runs(shadow: &str, markup: &[(usize, usize)]) -> Vec<Run> {
    let parsed = full_moon::parse_fallible(shadow, full_moon::LuaVersion::luau());

    if !parsed.errors().is_empty() {
        return Vec::new();
    }

    let block = parsed.ast().nodes();
    let mut runs: Vec<Run> = Vec::new();

    let statements = block
        .stmts()
        .filter_map(range_of)
        .chain(block.last_stmt().and_then(range_of));

    for (start, end) in statements {
        if markup.iter().any(|(from, to)| *from < end && start < *to) {
            continue;
        }

        // A statement that follows another with no markup between them joins
        // the run, and the comment between the two joins it as well.
        match runs.last_mut() {
            Some(run) if no_markup_between(markup, run.end, start) => run.end = end,
            _ => runs.push(Run { start, end }),
        }
    }

    runs
}

fn range_of(node: &impl Node) -> Option<(usize, usize)> {
    node.range()
        .map(|(start, end)| (start.bytes(), end.bytes()))
}

fn no_markup_between(markup: &[(usize, usize)], from: usize, to: usize) -> bool {
    !markup
        .iter()
        .any(|(start, _)| from <= *start && *start < to)
}

#[cfg(test)]
mod tests {
    use super::*;
    use luaux::Config;

    fn runs_of(src: &str) -> Vec<String> {
        let markup = crate::scan::segments(src, 0, src.len())
            .expect("a scan")
            .iter()
            .filter_map(|segment| match segment {
                crate::scan::Segment::Markup { start, end, .. } => Some((*start, *end)),
                crate::scan::Segment::Luau { .. } => None,
            })
            .collect::<Vec<_>>();

        let shadow = crate::shadow::view(src, &Config::default()).expect("a shadow");

        runs(&shadow, &markup)
            .iter()
            .map(|run| src[run.start..run.end].to_string())
            .collect()
    }

    #[test]
    fn a_file_of_luau_is_one_run() {
        assert_eq!(
            runs_of("local a = 1\nlocal b = 2\nreturn a\n"),
            ["local a = 1\nlocal b = 2\nreturn a"]
        );
    }

    #[test]
    fn a_comment_between_two_statements_stays_in_the_run() {
        assert_eq!(
            runs_of("local a = 1\n-- why\nlocal b = 2\n"),
            ["local a = 1\n-- why\nlocal b = 2"]
        );
    }

    #[test]
    fn a_statement_that_holds_markup_is_not_a_run() {
        // The `local` before and the `return` after are runs of their own, and
        // the function between them is the part that the worm lays out.
        assert_eq!(
            runs_of("local create = c\nlocal function App()\n\treturn <Frame/>\nend\nreturn App\n"),
            ["local create = c", "return App"]
        );
    }

    #[test]
    fn a_statement_that_is_markup_is_not_a_run() {
        assert_eq!(runs_of("local ui = <Frame/>\nreturn ui\n"), ["return ui"]);
    }

    #[test]
    fn a_file_that_does_not_parse_has_no_run() {
        assert!(runs("local function f(\n", &[]).is_empty());
    }
}
