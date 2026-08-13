//! A `{...}` hole, in a child position or in an attribute position.

use larvae_worm::native::Doc;
use luaux::markup::Span;

use super::Layout;
use super::node;
use crate::scan::{self, Segment};

/// The hole with its braces.
///
/// The whitespace that the author wrote inside the braces goes away, because
/// the markup parser drops it as well: `{ x }` and `{x}` are the same
/// expression to luaux. What takes its place is `space_inside_braces`, the
/// setting of larvae, because a brace of this worm reads like a brace of the
/// project beside it.
pub(super) fn doc(layout: &Layout, span: Span) -> Doc {
    let (start, end) = scan::hole_inner(layout.src, span);

    let space = if layout.options.space_inside_braces {
        Doc::lit(" ")
    } else {
        Doc::Nil
    };

    Doc::concat([
        Doc::lit("{"),
        space.clone(),
        content(layout, start, end),
        space,
        Doc::lit("}"),
    ])
}

/// What is inside a hole, which is one of three things.
///
/// A hole that holds Luau becomes a `host_expr` span, because larvae owns the
/// Luau printer. A hole that holds one markup node is markup, and the worm lays
/// it out. A hole that mixes the two keeps the bytes of the source: a part of a
/// mixed expression is not an expression on its own, so larvae cannot parse it,
/// and the worm does not format Luau. A hole that holds a comment keeps its
/// bytes as well, so that the comment lives.
fn content(layout: &Layout, start: usize, end: usize) -> Doc {
    let src = layout.src;
    let verbatim = Doc::src(start as u32, end as u32);

    // The Luau emitter of larvae drops a comment that stands beside an
    // expression, and larvae then refuses the whole file to save that comment.
    // The bytes of the hole cross instead, and the file formats.
    if scan::holds_comment(src, start, end) {
        return verbatim;
    }

    let Ok(segments) = scan::segments(src, start, end) else {
        return verbatim;
    };

    match segments.as_slice() {
        [
            Segment::Markup {
                node,
                start: from,
                end: to,
            },
        ] if *from == start && *to == end => node::doc(layout, node),

        parts
            if parts
                .iter()
                .all(|part| matches!(part, Segment::Luau { .. })) =>
        {
            Doc::host_expr(start as u32, end as u32)
        }

        _ => verbatim,
    }
}

#[cfg(test)]
mod tests {
    use super::super::Options;
    use super::super::tests::{json, json_under};

    #[test]
    fn a_hole_that_holds_luau_goes_to_larvae() {
        let doc = json("<Frame>{ items }</Frame>");

        // The whitespace of the author goes away with the trim, and the
        // setting of the project puts the space back.
        assert!(
            doc.contains(r#"{"lit":"{"},{"lit":" "},{"host":{"start":9,"end":14,"parse":"expr"}}"#),
            "{doc}"
        );
    }

    #[test]
    fn space_inside_braces_holds_the_braces_of_a_hole_too() {
        let tight = Options {
            space_inside_braces: false,
            ..Options::default()
        };
        let doc = json_under("<Frame>{items}</Frame>", &tight);

        assert!(
            doc.contains(r#"{"lit":"{"},"nil",{"host":{"start":8,"end":13,"parse":"expr"}}"#),
            "{doc}"
        );
    }

    #[test]
    fn a_hole_that_holds_markup_is_markup() {
        // larvae cannot parse markup, so this hole is not a `host` span.
        let doc = json("<Frame>{<Label/>}</Frame>");

        assert!(!doc.contains("host"), "{doc}");
        assert!(doc.contains(r#"{"lit":"<Label"}"#), "{doc}");
    }

    #[test]
    fn a_hole_that_holds_a_comment_keeps_its_bytes() {
        // larvae would drop the comment and then refuse the file to save it.
        let doc = json("<Frame Size={x --[[ wide ]]}/>");

        assert!(
            doc.contains(r#"{"lit":"{"},{"lit":" "},{"src":[13,27]},{"lit":" "},{"lit":"}"}"#),
            "{doc}"
        );
        assert!(!doc.contains("host"), "{doc}");
    }

    #[test]
    fn a_hole_that_mixes_luau_and_markup_keeps_its_bytes() {
        // `cond and ` is not an expression, so no part of this hole can go to
        // larvae. The bytes of the source cross without a change.
        let doc = json("<Frame>{cond and <Label/> or nil}</Frame>");

        assert!(
            doc.contains(r#"{"lit":"{"},{"lit":" "},{"src":[8,32]},{"lit":" "},{"lit":"}"}"#),
            "{doc}"
        );
    }
}
