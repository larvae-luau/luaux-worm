//! An element with no child that keeps a closing tag.

use larvae_worm::native::Finding;
use luaux::markup::Node;

/// The name that `worm.toml` declares for this rule.
pub const NAME: &str = "luaux_self_closing_element";

/// Reports `<Frame></Frame>`, which is `<Frame/>` with more to read.
///
/// This is the `useSelfClosingElements` rule of Biome. The markup parser drops
/// a run of whitespace between the two tags, so `<Frame>\n</Frame>` has no
/// child either and closes itself as well.
pub fn check(src: &str, node: &Node, out: &mut Vec<Finding>) {
    let Node::Element(element) = node else {
        return;
    };

    if !element.children.is_empty() {
        return;
    }

    // `<Frame/>` already closes itself. The tree cannot tell the two apart,
    // because both hold no child, so the source answers.
    if src[element.span.start..element.span.end].ends_with("/>") {
        return;
    }

    let name = element.name.as_written();

    out.push(
        Finding::new(
            NAME,
            super::range(element.span),
            format!("<{name}> has no child, so it can close itself"),
        )
        .with_help(format!("write <{name} /> and remove the closing tag")),
    );
}

#[cfg(test)]
mod tests {
    use super::super::tests::{findings_named, text};
    use super::*;

    #[test]
    fn reports_an_empty_element_with_a_closing_tag() {
        let src = "return <Frame></Frame>";
        let findings = findings_named(src, NAME);

        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(text(src, &findings[0]), "<Frame></Frame>");
        assert!(findings[0].help.is_some());
    }

    #[test]
    fn reports_an_element_that_holds_whitespace_only() {
        // The parser drops that whitespace, so the element loses nothing.
        assert_eq!(findings_named("return <Frame>\n\t</Frame>", NAME).len(), 1);
    }

    #[test]
    fn an_element_that_closes_itself_is_not_a_finding() {
        assert!(findings_named("return <Frame/>", NAME).is_empty());
        assert!(findings_named("return <Frame Size={1} />", NAME).is_empty());
    }

    #[test]
    fn an_element_with_a_child_is_not_a_finding() {
        assert!(findings_named("return <Frame><A/></Frame>", NAME).is_empty());
        assert!(findings_named("return <Frame>text</Frame>", NAME).is_empty());
        assert!(findings_named("return <Frame><!-- note --></Frame>", NAME).is_empty());
    }
}
