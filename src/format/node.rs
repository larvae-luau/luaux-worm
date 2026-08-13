//! An element, a fragment, a tag, and an attribute.
//!
//! The layout follows the layout that Prettier and Biome give to JSX, because a
//! reader of `.luaux` reads that shape in other languages as well. The tag is
//! flat while it fits the line. It breaks with one attribute per line, and the
//! closing bracket goes at the left, under the name:
//!
//! ```text
//! <TextLabel
//!     Text="hello"
//!     TextSize={18}
//! >
//!     the children
//! </TextLabel>
//! ```
//!
//! Three options of [`super::options`] work here. `luaux_bracket_same_line`
//! moves that bracket up to the last attribute, `luaux_attribute_per_line`
//! breaks a tag that would fit, and `luaux_attribute_quotes` decides the
//! quotes of a string.

use larvae_worm::native::Doc;
use luaux::markup::{Attribute, AttributeValue, Child, Element, Fragment, Node};

use super::Layout;
use super::children;
use super::hole;
use crate::scan;

pub(super) fn doc(layout: &Layout, node: &Node) -> Doc {
    match node {
        Node::Element(element) => element_doc(layout, element),
        Node::Fragment(fragment) => fragment_doc(layout, fragment),
    }
}

/// `<Frame ... />` or `<Frame ...>children</Frame>`.
fn element_doc(layout: &Layout, element: &Element) -> Doc {
    let name = element.name.as_written();

    if element.children.is_empty() {
        return self_closing(layout, format!("<{name}"), &element.attributes);
    }

    open_and_close(
        layout,
        format!("<{name}"),
        &element.attributes,
        &element.children,
        format!("</{name}>"),
    )
}

/// `<>children</>`. A fragment has no name and no attribute.
fn fragment_doc(layout: &Layout, fragment: &Fragment) -> Doc {
    open_and_close(
        layout,
        String::from("<"),
        &[],
        &fragment.children,
        String::from("</>"),
    )
}

/// An element with no child.
///
/// The tag holds one space before the slash, and `luaux_self_closing_space`
/// takes it away. The space becomes a line break when the attributes do not fit the
/// line.
fn self_closing(layout: &Layout, open: String, attributes: &[Attribute]) -> Doc {
    let space = match (layout.options.self_closing_space, attributes.is_empty()) {
        // With no attribute there is nothing to break, so the space stays a
        // space.
        (true, true) => Doc::lit(" "),
        (true, false) => Doc::Line,
        (false, true) => Doc::Nil,
        (false, false) => Doc::Soft,
    };

    Doc::group(Doc::concat([
        Doc::lit(open),
        attributes_doc(layout, attributes),
        space,
        Doc::lit("/>"),
    ]))
}

/// An element or a fragment with children.
///
/// The tag is a group of its own, so the attributes stay on one line while the
/// children take more than one.
fn open_and_close(
    layout: &Layout,
    open: String,
    attributes: &[Attribute],
    children: &[Child],
    close: String,
) -> Doc {
    let (pieces, before_close) = children::content(layout, children);

    let mut inside = Vec::new();

    for piece in pieces {
        inside.push(piece.before);
        inside.push(piece.doc);
    }

    // `luaux_bracket_same_line` puts the `>` after the last attribute, and the
    // default puts it on a line of its own, under the name of the element.
    let before_bracket = if layout.options.bracket_same_line {
        Doc::Nil
    } else {
        Doc::Soft
    };

    Doc::group(Doc::concat([
        Doc::group(Doc::concat([
            Doc::lit(open),
            attributes_doc(layout, attributes),
            before_bracket,
            Doc::lit(">"),
        ])),
        Doc::indent(Doc::concat(inside)),
        before_close,
        Doc::lit(close),
    ]))
}

/// The attributes, one line each when the tag breaks.
fn attributes_doc(layout: &Layout, attributes: &[Attribute]) -> Doc {
    if attributes.is_empty() {
        return Doc::Nil;
    }

    // `luaux_attribute_per_line` gives one attribute per line at all times, in
    // the same way as `singleAttributePerLine` of Prettier. A hard break asks
    // for that, because it breaks the tag around it as well.
    let before = if layout.options.attribute_per_line && attributes.len() > 1 {
        Doc::Hard
    } else {
        Doc::Line
    };

    let mut parts = Vec::new();

    for attribute in attributes {
        parts.push(before.clone());
        parts.push(attribute_doc(layout, attribute));
    }

    Doc::indent(Doc::concat(parts))
}

/// One attribute. The order never changes, because the last value of a name
/// wins in luaux and in Vide.
fn attribute_doc(layout: &Layout, attribute: &Attribute) -> Doc {
    match attribute {
        Attribute::Named { name, value, span } => match value {
            // `Visible` is the shorthand for `Visible={true}`, and it stays as
            // the author wrote it.
            AttributeValue::Boolean => Doc::lit(name.clone()),

            AttributeValue::StringLiteral(literal) => Doc::concat([
                Doc::lit(format!("{name}=")),
                string(layout, span.end - literal.len(), span.end),
            ]),

            AttributeValue::Expression(_) => Doc::concat([
                Doc::lit(format!("{name}=")),
                hole::doc(layout, scan::brace_span(layout.src, *span)),
            ]),
        },

        // `{props}` in attribute position.
        Attribute::Spread { span, .. } => hole::doc(layout, *span),
    }
}

/// The string of an attribute, under `luaux_attribute_quotes`.
///
/// The text between the quotes crosses as a span, byte for byte, because it is
/// an ordinary Luau string and holds its own escapes. Only the two quotes at
/// the ends change, and they change only where the text holds no quote of the
/// kind that the rule asks for. To add an escape to the text is to write the
/// string again, and a formatter that writes a string again writes a bug one
/// day.
fn string(layout: &Layout, start: usize, end: usize) -> Doc {
    let span = Doc::src(start as u32, end as u32);

    let Some(wanted) = layout.options.attribute_quotes.quote() else {
        return span;
    };

    let bytes = layout.src.as_bytes();

    if bytes[start] == wanted || holds_quote(&bytes[start + 1..end - 1], wanted) {
        return span;
    }

    let quote = Doc::lit((wanted as char).to_string());

    Doc::concat([
        quote.clone(),
        Doc::src((start + 1) as u32, (end - 1) as u32),
        quote,
    ])
}

/// Whether the text holds this quote outside an escape.
fn holds_quote(text: &[u8], quote: u8) -> bool {
    let mut index = 0;

    while index < text.len() {
        // A backslash takes the byte after it, whatever that byte is.
        if text[index] == b'\\' {
            index += 2;
            continue;
        }

        if text[index] == quote {
            return true;
        }

        index += 1;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::super::options::{Options, QuoteStyle};
    use super::super::tests::{json, json_under};

    #[test]
    fn an_element_with_no_child_closes_itself_with_a_space() {
        assert_eq!(
            json("<Frame/>"),
            r#"{"concat":[{"group":{"concat":[{"lit":"<Frame"},"nil",{"lit":" "},{"lit":"/>"}]}}]}"#
        );
    }

    #[test]
    fn an_attribute_value_is_a_host_expression() {
        let doc = json(r#"<Frame Size={UDim2.new(1, 0)} Visible/>"#);

        assert!(
            doc.contains(
                r#"{"lit":"Size="},{"concat":[{"lit":"{"},{"lit":" "},{"host":{"start":13,"end":28"#
            ),
            "{doc}"
        );
        assert!(doc.contains(r#""parse":"expr""#), "{doc}");
        assert!(doc.contains(r#"{"lit":"Visible"}"#), "{doc}");
    }

    #[test]
    fn each_attribute_takes_one_line_when_the_tag_breaks() {
        let doc = json(r#"<Frame A={1} B={2}/>"#);

        // One `line` before each attribute, and one before the slash.
        assert_eq!(doc.matches(r#""line""#).count(), 3, "{doc}");
        assert!(doc.contains(r#"{"indent":{"concat":["line""#), "{doc}");
    }

    #[test]
    fn a_spread_keeps_its_braces() {
        let doc = json("<Frame {props}/>");

        assert!(
            doc.contains(
                r#"{"lit":"{"},{"lit":" "},{"host":{"start":8,"end":13,"parse":"expr"}},{"lit":" "}"#
            ),
            "{doc}"
        );
    }

    #[test]
    fn a_fragment_has_no_name() {
        let doc = json("<><A/></>");

        assert!(doc.contains(r#"{"lit":"<"}"#), "{doc}");
        assert!(doc.contains(r#"{"lit":"</>"}"#), "{doc}");
    }

    #[test]
    fn attribute_quotes_give_a_string_the_quotes_of_the_project() {
        // The text between the quotes stays a span, and the quotes are new.
        let doc = json("<Frame Name='root'/>");

        assert!(
            doc.contains(r#"{"lit":"\""},{"src":[13,17]},{"lit":"\""}"#),
            "{doc}"
        );

        let single = Options {
            attribute_quotes: QuoteStyle::Single,
            ..Options::default()
        };

        assert!(
            json_under(r#"<Frame Name="root"/>"#, &single)
                .contains(r#"{"lit":"'"},{"src":[13,17]},{"lit":"'"}"#),
            "{doc}"
        );
    }

    #[test]
    fn attribute_quotes_leave_a_string_that_holds_the_other_quote() {
        // `Name='say "hi"'` under double quotes would need a new escape, and to
        // write the string again is to change what it holds.
        let doc = json(r#"<Frame Name='say "hi"'/>"#);

        // The literal crosses with its own quotes, from the `'` to the `'`.
        assert!(doc.contains(r#"{"src":[12,22]}"#), "{doc}");

        // An escape of that quote is not the quote, so this one does change.
        let escaped = json(r#"<Frame Name='say \"hi\"'/>"#);

        assert!(
            escaped.contains(r#"{"lit":"\""},{"src":[13,23]},{"lit":"\""}"#),
            "{escaped}"
        );
    }

    #[test]
    fn attribute_quotes_preserve_keeps_the_quotes_of_the_author() {
        let options = Options {
            attribute_quotes: QuoteStyle::Preserve,
            ..Options::default()
        };
        let doc = json_under("<Frame Name='root'/>", &options);

        assert!(doc.contains(r#"{"src":[12,18]}"#), "{doc}");
    }

    #[test]
    fn bracket_same_line_puts_the_bracket_after_the_last_attribute() {
        let options = Options {
            bracket_same_line: true,
            ..Options::default()
        };
        let doc = json_under("<Frame A={1}><B/></Frame>", &options);

        // No `soft` before the `>` of the tag: one for the child, and one
        // before the closing tag.
        assert_eq!(doc.matches(r#""soft""#).count(), 2, "{doc}");
    }

    #[test]
    fn attribute_per_line_breaks_a_tag_that_would_fit() {
        let options = Options {
            attribute_per_line: true,
            ..Options::default()
        };
        let doc = json_under("<Frame A={1} B={2}/>", &options);

        // A hard break before each attribute breaks the tag around them.
        assert_eq!(doc.matches(r#""hard""#).count(), 2, "{doc}");

        // One attribute is still one line.
        assert!(!json_under("<Frame A={1}/>", &options).contains("hard"));
    }

    #[test]
    fn self_closing_space_takes_the_space_away() {
        let options = Options {
            self_closing_space: false,
            ..Options::default()
        };

        assert!(
            json_under("<Frame/>", &options).contains(r#""nil",{"lit":"/>"}"#),
            "the space is gone"
        );
        assert!(
            json_under("<Frame A={1}/>", &options).contains(r#""soft",{"lit":"/>"}"#),
            "the break is still there"
        );
    }
}
