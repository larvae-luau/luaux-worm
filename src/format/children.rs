//! The content of an element, and where a line break is safe.
//!
//! The markup parser drops a run of whitespace between two children, so the
//! worm decides where a break goes. A break is safe where the source held a
//! newline or nothing. A break is not safe where the source held a space,
//! because that space is text: `Name: {x}` over two lines becomes `Name:{x}`
//! and loses the space.
//!
//! Each child that is not text takes one line when the element breaks, in the
//! same way as Prettier and Biome lay out a JSX child. Text is different: it
//! fills the line. [`super::text`] holds that part.

use larvae_worm::native::Doc;
use luaux::markup::Child;

use super::Layout;
use super::hole;
use super::node;
use super::text;

/// One part of the content that a line break must not split.
pub(super) struct Piece {
    /// What comes between the part before this one and this one
    pub before: Doc,
    pub doc: Doc,
}

/// The children of an element, and the separator before the closing tag.
pub(super) fn content(layout: &Layout, children: &[Child]) -> (Vec<Piece>, Doc) {
    let src = layout.src;
    let mut pieces: Vec<Piece> = Vec::new();

    // The text before this point ends with a space that is text.
    let mut glue = false;
    // The part before this point holds a line break of its own.
    let mut broken = false;
    let mut previous_end: Option<usize> = None;

    for child in children {
        let span = child.span();

        let before = if glue {
            Doc::Nil
        } else if broken {
            Doc::Hard
        } else if layout.options.blank_lines && blank_between(src, previous_end, span.start) {
            Doc::Blank
        } else {
            // Nothing when flat: two children with no text between them have no
            // space between them, and a space there becomes text.
            Doc::Soft
        };

        match child {
            Child::Text { .. } => {
                let run = text::run(layout, span);

                pieces.push(Piece {
                    before: if run.sticky_left { Doc::Nil } else { before },
                    doc: run.doc(layout.options.text_wrap),
                });

                glue = run.sticky_right;
                broken = false;
            }

            Child::Node(node) => {
                pieces.push(Piece {
                    before,
                    doc: node::doc(layout, node),
                });

                glue = false;
                broken = false;
            }

            Child::Expression { .. } => {
                pieces.push(Piece {
                    before,
                    doc: hole::doc(layout, span),
                });

                glue = false;
                broken = false;
            }

            // A comment crosses as a span. larvae refuses a layout that lost a
            // comment, and it compares the bytes, so the worm changes nothing
            // inside one.
            Child::Comment { .. } => {
                let holds_a_line = src[span.start..span.end].contains('\n');

                let before = if glue {
                    Doc::Nil
                } else if holds_a_line {
                    // A comment over two lines gets a line of its own, because
                    // its own newlines break the line in both modes anyway.
                    Doc::Hard
                } else {
                    before
                };

                pieces.push(Piece {
                    before,
                    doc: Doc::src(span.start as u32, span.end as u32),
                });

                glue = false;
                broken = holds_a_line;
            }
        }

        previous_end = Some(span.end);
    }

    let before_close = if glue {
        Doc::Nil
    } else if broken {
        Doc::Hard
    } else {
        Doc::Soft
    };

    (pieces, before_close)
}

/// Whether the author left a blank line between two children.
fn blank_between(src: &str, from: Option<usize>, to: usize) -> bool {
    from.is_some_and(|from| from <= to && src[from..to].matches('\n').count() >= 2)
}

#[cfg(test)]
mod tests {
    use super::super::tests::json;

    #[test]
    fn children_break_one_by_one() {
        let doc = json("<Frame><A/><B/></Frame>");

        // One `soft` before the `>` of the tag, one before each child, and one
        // before the closing tag. Two children with no text between them hold
        // no space between them, so a flat layout adds nothing.
        assert_eq!(doc.matches(r#""soft""#).count(), 4);
        assert!(doc.contains(r#"{"lit":"</Frame>"}"#), "{doc}");
    }

    #[test]
    fn a_text_run_keeps_the_space_that_is_text() {
        // `Name: ` ends with a space that the reader sees, so the hole after it
        // stays on the same line: the separator is `nil`, and not `soft`.
        let doc = json("<Label>Name: {name}</Label>");

        assert!(doc.contains(r#""nil",{"concat":[{"lit":"{"}"#), "{doc}");
    }

    #[test]
    fn a_blank_line_between_two_children_stays() {
        let doc = json("<Frame>\n\t<A/>\n\n\t<B/>\n</Frame>");

        assert_eq!(doc.matches(r#""blank""#).count(), 1);
    }

    #[test]
    fn a_comment_crosses_as_a_span() {
        let doc = json("<Frame><!-- note --></Frame>");

        assert!(doc.contains(r#"{"src":[7,20]}"#), "{doc}");
    }

    #[test]
    fn a_comment_over_two_lines_takes_a_line_of_its_own() {
        let doc = json("<Frame><!-- one\ntwo --><A/></Frame>");

        // `hard` before the comment, and `hard` before the child after it.
        assert_eq!(doc.matches(r#""hard""#).count(), 2, "{doc}");
    }
}
