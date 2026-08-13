//! The layout document of a `.luaux` file.
//!
//! larvae owns the printer. This module returns a tree of layout instructions,
//! and larvae renders the tree with the width and the indentation of the
//! project. Thus a `.luaux` file breaks a line in the same way as a `.luau`
//! file, and the worm holds no printer of its own.
//!
//! Two rules control every module below:
//!
//! - Ordinary Luau becomes a `host` span. larvae parses the span and formats
//!   it. The worm formats markup only.
//! - Every part of the markup comes from a span of the source. The luaux syntax
//!   tree decodes an escape and joins the lines of a text run while it parses,
//!   so the tree cannot give back the text that the author wrote.
//!
//! | File | What it lays out |
//! |---|---|
//! | `mod.rs` | The file: `host` spans, markup nodes, and the space between them |
//! | [`node`] | An element, a fragment, a tag, and an attribute |
//! | [`children`] | The content of an element, and where a break is safe |
//! | [`text`] | A text run, and the whitespace rule of luaux |
//! | [`hole`] | A `{...}` hole |
//! | [`options`] | The rules of the layout that a project sets |

mod children;
mod hole;
mod node;
pub mod options;
mod text;

use larvae_worm::native::{Doc, Format};
use luaux::Config;

use crate::report;
use crate::scan::{self, Segment};
use crate::shadow;
use crate::statements;

pub use options::Options;

/// The source and the rules, which every part of the layout reads.
///
/// The two travel together, because a span means nothing without the text that
/// it points into, and a rule means nothing without a span to apply it to.
pub(super) struct Layout<'a> {
    pub src: &'a str,
    pub options: &'a Options,
}

/// The layout of the whole file, and the span of every comment in it.
pub fn format(src: &str, options: &Options) -> Result<Format, String> {
    let layout = Layout { src, options };

    Ok(Format::document(document(&layout)?).with_comments(scan::marks(src).comments))
}

/// The layout of the whole file.
///
/// larvae parses every `host` span on its own and refuses one that opens a
/// block and does not close it, so a span holds whole statements and nothing
/// less. [`crate::statements`] names the runs of statements that hold no
/// markup, and each of those goes to larvae. Everything else crosses byte for
/// byte, with the markup inside it laid out by this module.
///
/// So a `.luaux` file gets the style of the project for the Luau that stands on
/// its own, and the worm changes nothing about the statement that holds the
/// markup except the markup itself. larvae adds the newline at the end of the
/// file, so the walk drops the whitespace at each end.
fn document(layout: &Layout) -> Result<Doc, String> {
    let src = layout.src;
    let segments = scan::segments(src, 0, src.len())
        .map_err(|error| report::at(src, &error.message, error.offset, None))?;

    let markup: Vec<(usize, usize)> = segments
        .iter()
        .filter_map(|segment| match segment {
            Segment::Markup { start, end, .. } => Some((*start, *end)),
            Segment::Luau { .. } => None,
        })
        .collect();

    // The shadow is Luau at the offsets of the source, so a parse of it says
    // where each statement ends. The settings of the compiler change nothing
    // about that, so the default ones answer here.
    let runs = shadow::view(src, &Config::default())
        .map(|shadow| statements::runs(&shadow, &markup))
        .unwrap_or_default();

    let (start, end) = scan::trimmed(src, 0, src.len());
    let mut parts = Vec::new();
    let mut cursor = start;

    for run in &runs {
        if run.start < cursor || run.end > end {
            continue;
        }

        verbatim(layout, cursor, run.start, &segments, &mut parts);
        parts.push(Doc::host(run.start as u32, run.end as u32));
        cursor = run.end;
    }

    verbatim(layout, cursor, end, &segments, &mut parts);

    Ok(Doc::concat(parts))
}

/// One range of the file that larvae does not format.
///
/// The Luau of the range crosses byte for byte, because a part of a statement
/// is not a statement and larvae cannot parse it. The markup inside the range
/// is the worm's own work, and it is laid out.
fn verbatim(layout: &Layout, from: usize, to: usize, segments: &[Segment], parts: &mut Vec<Doc>) {
    let mut cursor = from;

    for segment in segments {
        let Segment::Markup { node, start, end } = segment else {
            continue;
        };

        if *start < from || *end > to {
            continue;
        }

        if cursor < *start {
            parts.push(Doc::src(cursor as u32, *start as u32));
        }

        parts.push(indented(layout, *start, node::doc(layout, node)));
        cursor = *end;
    }

    if cursor < to {
        parts.push(Doc::src(cursor as u32, to as u32));
    }
}

/// The node at the depth of the line that it starts on.
///
/// The Luau around a node crosses byte for byte, so that line already holds the
/// indentation that the author wrote. A break inside the node has to start from
/// that depth, and one `Doc::indent` for each level is how a document says so.
/// The break sits inside the indent, and not beside it, or the printer raises
/// nothing.
fn indented(layout: &Layout, start: usize, node: Doc) -> Doc {
    let mut doc = node;

    for _ in 0..depth(layout, start) {
        doc = Doc::indent(doc);
    }

    doc
}

/// How many levels of indentation the line of this offset holds.
fn depth(layout: &Layout, start: usize) -> usize {
    let src = layout.src;
    let line = src[..start].rfind('\n').map_or(0, |at| at + 1);
    let indent = &src[line..start];
    let indent = &indent[..indent.len() - indent.trim_start().len()];

    // A tab is one level. Spaces are one level for each `indent_width` of them,
    // which is the setting that larvae resolved for this project.
    indent.matches('\t').count() + indent.matches(' ').count() / layout.options.indent_width.max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The JSON of the document under the default rules, which is the contract
    /// with larvae.
    pub(super) fn json(src: &str) -> String {
        json_under(src, &Options::default())
    }

    /// The same, for a test of one rule of the layout.
    pub(super) fn json_under(src: &str, options: &Options) -> String {
        let layout = Layout { src, options };

        serde_json::to_string(&document(&layout).expect("a document")).expect("json")
    }

    #[test]
    fn a_file_with_no_markup_is_one_host_span() {
        assert_eq!(
            json("local x = 1\n"),
            r#"{"concat":[{"host":{"start":0,"end":11,"parse":"block"}}]}"#
        );
    }

    #[test]
    fn the_statement_that_holds_markup_crosses_byte_for_byte() {
        // `return ` is the front of a statement, and larvae parses no such
        // thing, so it crosses as a span of the source and not as `host`.
        assert_eq!(
            json("return <Frame/>"),
            r#"{"concat":[{"src":[0,7]},"#.to_owned()
                + r#"{"group":{"concat":[{"lit":"<Frame"},"nil",{"lit":" "},{"lit":"/>"}]}}]}"#
        );
    }

    #[test]
    fn a_statement_that_holds_no_markup_goes_to_larvae() {
        // The first statement holds markup and the second does not.
        let doc = json("local x = <Frame/>\nlocal y = 2\n");

        assert!(doc.contains(r#"{"src":[0,10]}"#), "{doc}");
        assert!(
            doc.contains(r#"{"host":{"start":19,"end":30,"parse":"block"}}"#),
            "{doc}"
        );
    }

    #[test]
    fn a_node_lands_at_the_depth_of_the_line_that_it_starts_on() {
        // One tab before the node, so one `indent` around it. A break inside
        // the node then starts from the depth of the author.
        let doc = json("local x =\n\t<Frame>{a}</Frame>\n");

        assert!(
            doc.contains(r#"{"src":[0,11]},{"indent":{"group":"#),
            "{doc}"
        );
    }

    #[test]
    fn the_whitespace_at_each_end_of_the_file_goes_away() {
        // larvae adds the newline at the end of the file.
        let doc = json("\n\nreturn <Frame/>\n\n\n");

        assert!(doc.starts_with(r#"{"concat":[{"src":[2,9]}"#), "{doc}");
        assert!(doc.ends_with(r#"{"lit":"/>"}]}}]}"#), "{doc}");
    }
}
