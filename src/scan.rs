//! Where the markup is, and where the Luau is.
//!
//! A `.luaux` file holds two languages. The luaux crate decides which `<` opens
//! markup, and this module turns that decision into a list of parts. The
//! formatter lays out the markup parts and gives the Luau parts to larvae.
//!
//! The lexer and the markup parser run one after the other, in the same way as
//! the luaux compiler runs them. A `.luaux` file is not Luau from the first byte
//! to the last, so a lexer over the whole file fails on markup text. The lexer
//! stops at each markup site, the markup parser reads the node, and the lexer
//! starts again after the node.

use luaux::lexer::{Lexer, TokenKind};
use luaux::markup::{Attribute, AttributeValue, Child, Node, Span, parse_node};
use luaux::markup_scan::Scanner;

/// One part of a range of the source.
pub enum Segment {
    /// Ordinary Luau. larvae formats it, so the worm keeps the range only.
    Luau { start: usize, end: usize },
    /// One markup node, and the range that the node covers.
    Markup {
        node: Node,
        start: usize,
        end: usize,
    },
}

/// A message about a source that neither the lexer nor the markup parser reads.
#[derive(Debug)]
pub struct ScanError {
    pub message: String,
    pub offset: usize,
}

/// Splits `src[start..end]` into Luau parts and markup nodes, in source order.
///
/// The offsets in the result are offsets into `src`, and not into the range,
/// because a layout document and a finding both speak in file offsets.
pub fn segments(src: &str, start: usize, end: usize) -> Result<Vec<Segment>, ScanError> {
    let mut lexer = Lexer::at(src, start);
    let mut scanner = Scanner::new(src);
    let mut segments = Vec::new();
    let mut cursor = start;

    while let Some(token) = lexer.next_token() {
        let token = token.map_err(|error| ScanError {
            message: error.message,
            offset: error.offset,
        })?;

        if token.start >= end {
            break;
        }

        // The scanner needs the tokens after this one to tell a type from an
        // expression, so it gets a lexer that starts at the end of this token.
        let lookahead = Lexer::at(src, token.end);

        if !scanner.feed(token, &lookahead) {
            continue;
        }

        let (node, node_end) = parse_node(src, token.start).map_err(|error| ScanError {
            message: error.message,
            offset: error.offset,
        })?;

        if cursor < token.start {
            segments.push(Segment::Luau {
                start: cursor,
                end: token.start,
            });
        }

        segments.push(Segment::Markup {
            node,
            start: token.start,
            end: node_end,
        });

        cursor = node_end;
        lexer.seek(node_end);
        scanner.note_luaux_region();
    }

    if cursor < end {
        segments.push(Segment::Luau { start: cursor, end });
    }

    Ok(segments)
}

/// The range inside the braces of a `{...}` hole, without the whitespace at
/// each end.
///
/// The markup parser trims the same whitespace before it captures the
/// expression, so this range holds the exact text that the compiler reads.
pub fn hole_inner(src: &str, span: Span) -> (usize, usize) {
    trimmed(src, span.start + 1, span.end - 1)
}

/// The range without the whitespace at each end. An empty range keeps `start`.
pub fn trimmed(src: &str, start: usize, end: usize) -> (usize, usize) {
    let text = &src[start..end];
    let front = text.len() - text.trim_start().len();
    let back = text.len() - text.trim_end().len();

    if front + back >= text.len() {
        return (start, start);
    }

    (start + front, end - back)
}

/// What one walk of the whole file finds.
#[derive(Default)]
pub struct Marks {
    /// The span of every comment, in source order.
    ///
    /// larvae reads these spans two times. It refuses a layout that lost a
    /// comment, and it applies `-- larvae: allow(...)` over a finding. A
    /// comment that this list does not name loses its power to hide a finding,
    /// so the walk goes into every hole, and not over the markup only.
    pub comments: Vec<(u32, u32)>,

    /// Where the expression of each `{...}` hole starts.
    ///
    /// luaux compiles the expression inside a hole as a source of its own, so a
    /// finding from inside a hole counts from the start of that expression.
    /// [`crate::lints`] adds one of these offsets to put such a finding back in
    /// the file.
    pub holes: Vec<usize>,
}

/// Walks the whole file, and reports the comments and the holes in it.
pub fn marks(src: &str) -> Marks {
    let mut marks = Marks::default();
    collect(src, 0, src.len(), &mut marks);

    marks.comments.sort_unstable();
    marks.holes.sort_unstable();

    marks
}

fn collect(src: &str, start: usize, end: usize, out: &mut Marks) {
    let Ok(segments) = segments(src, start, end) else {
        // A source that does not scan has no format and no lint result either.
        // The caller reports that, and this walk stops without a message.
        return;
    };

    for segment in &segments {
        match segment {
            Segment::Luau { start, end } => luau_comments(src, *start, *end, out),
            Segment::Markup { node, .. } => markup_marks(src, node, out),
        }
    }
}

/// The comments of one run of ordinary Luau.
fn luau_comments(src: &str, start: usize, end: usize, out: &mut Marks) {
    let mut lexer = Lexer::at(src, start);

    while let Some(Ok(token)) = lexer.next_token() {
        if token.start >= end {
            break;
        }

        if token.kind == TokenKind::Comment {
            out.comments.push((token.start as u32, token.end as u32));
        }
    }
}

/// Everything inside one markup node, and inside every hole in it.
fn markup_marks(src: &str, node: &Node, out: &mut Marks) {
    let (attributes, children) = match node {
        Node::Element(element) => (element.attributes.as_slice(), element.children.as_slice()),
        Node::Fragment(fragment) => (&[][..], fragment.children.as_slice()),
    };

    for attribute in attributes {
        match attribute {
            Attribute::Named {
                value: AttributeValue::Expression(_),
                span,
                ..
            } => hole_marks(src, brace_span(src, *span), out),

            Attribute::Spread { span, .. } => hole_marks(src, *span, out),

            Attribute::Named { .. } => {}
        }
    }

    for child in children {
        match child {
            Child::Comment { span, .. } => out.comments.push((span.start as u32, span.end as u32)),

            Child::Expression { span, .. } => hole_marks(src, *span, out),

            Child::Node(node) => markup_marks(src, node, out),

            Child::Text { .. } => {}
        }
    }
}

/// The expression inside one hole, which is a source of its own to luaux.
fn hole_marks(src: &str, span: Span, out: &mut Marks) {
    let (start, end) = hole_inner(src, span);

    out.holes.push(start);
    collect(src, start, end, out);
}

/// Whether this range of the source holds a comment.
///
/// larvae formats a `host_expr` span with its own emitter, and that emitter
/// drops a comment that follows the expression. The backstop of larvae then
/// refuses the whole file, so the worm keeps the bytes of such a hole instead.
pub fn holds_comment(src: &str, start: usize, end: usize) -> bool {
    let mut lexer = Lexer::at(src, start);

    while let Some(Ok(token)) = lexer.next_token() {
        if token.start >= end {
            break;
        }

        if token.kind == TokenKind::Comment {
            return true;
        }
    }

    false
}

/// Calls `visit` for every markup node in the file.
///
/// The walk goes into the children of a node and into every hole, so a rule
/// sees each node one time, wherever the author wrote it. Each node carries the
/// spans of the file, so a finding from a rule needs no repair.
pub fn each_node(src: &str, visit: &mut impl FnMut(&Node)) {
    each_node_in(src, 0, src.len(), visit);
}

fn each_node_in(src: &str, start: usize, end: usize, visit: &mut impl FnMut(&Node)) {
    let Ok(segments) = segments(src, start, end) else {
        return;
    };

    for segment in &segments {
        if let Segment::Markup { node, .. } = segment {
            node_and_holes(src, node, visit);
        }
    }
}

fn node_and_holes(src: &str, node: &Node, visit: &mut impl FnMut(&Node)) {
    visit(node);

    let (attributes, children) = match node {
        Node::Element(element) => (element.attributes.as_slice(), element.children.as_slice()),
        Node::Fragment(fragment) => (&[][..], fragment.children.as_slice()),
    };

    for attribute in attributes {
        let span = match attribute {
            Attribute::Named {
                value: AttributeValue::Expression(_),
                span,
                ..
            } => brace_span(src, *span),

            Attribute::Spread { span, .. } => *span,

            Attribute::Named { .. } => continue,
        };

        let (start, end) = hole_inner(src, span);
        each_node_in(src, start, end, visit);
    }

    for child in children {
        match child {
            Child::Node(node) => node_and_holes(src, node, visit),

            Child::Expression { span, .. } => {
                let (start, end) = hole_inner(src, *span);
                each_node_in(src, start, end, visit);
            }

            Child::Comment { .. } | Child::Text { .. } => {}
        }
    }
}

/// The span of the `{...}` part of an attribute such as `Size={x}`.
///
/// The span of the attribute starts at the name, and the parser puts its end
/// one byte after the closing brace. The opening brace is the first one after
/// the name, because a name holds letters, digits, and an underscore only.
pub fn brace_span(src: &str, attribute: Span) -> Span {
    let text = &src[attribute.start..attribute.end];
    let open = attribute.start + text.find('{').unwrap_or(0);

    Span::new(open, attribute.end)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<String> {
        segments(src, 0, src.len())
            .expect("scan")
            .iter()
            .map(|segment| match segment {
                Segment::Luau { start, end } => format!("luau:{}", &src[*start..*end]),
                Segment::Markup { start, end, .. } => format!("markup:{}", &src[*start..*end]),
            })
            .collect()
    }

    #[test]
    fn splits_luau_from_markup() {
        assert_eq!(
            kinds("local ui = <Frame/>\n"),
            ["luau:local ui = ", "markup:<Frame/>", "luau:\n"]
        );
    }

    #[test]
    fn reads_a_file_that_holds_no_markup() {
        assert_eq!(kinds("local x = 1 < 2\n"), ["luau:local x = 1 < 2\n"]);
    }

    #[test]
    fn reads_markup_text_that_is_not_luau() {
        // An apostrophe opens a string in Luau and is a letter in markup text.
        // A lexer over the whole file fails here, and the segment walk does not.
        assert_eq!(
            kinds("return <Label>don't stop</Label>"),
            ["luau:return ", "markup:<Label>don't stop</Label>"]
        );
    }

    fn comment_text(src: &str) -> Vec<&str> {
        marks(src)
            .comments
            .iter()
            .map(|(start, end)| &src[*start as usize..*end as usize])
            .collect()
    }

    #[test]
    fn finds_the_comments_of_both_languages() {
        let src = "-- head\nreturn <Frame>\n<!-- note -->\n{--[[ hole ]]}\n</Frame>";

        assert_eq!(
            comment_text(src),
            ["-- head", "<!-- note -->", "{--[[ hole ]]}"]
        );
    }

    #[test]
    fn finds_a_comment_inside_a_hole() {
        let src = "return <Frame Size={x --[[ wide ]]}/>";

        assert_eq!(comment_text(src), ["--[[ wide ]]"]);
    }

    #[test]
    fn finds_where_the_expression_of_each_hole_starts() {
        let src = "return <Frame Size={x}>{items}</Frame>";

        assert_eq!(marks(src).holes, [20, 24]);
        assert_eq!(&src[20..21], "x");
        assert_eq!(&src[24..29], "items");
    }

    #[test]
    fn trims_a_hole_to_its_expression() {
        let src = "{ items }";
        assert_eq!(hole_inner(src, Span::new(0, src.len())), (2, 7));
    }
}
