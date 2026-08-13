//! A fragment that stands between a node and its children.

use larvae_worm::native::Finding;
use luaux::markup::{Child, Node};

/// The name that `worm.toml` declares for this rule.
pub const NAME: &str = "luaux_useless_fragment";

/// Reports the `<>` of `<Frame><><A/><B/></></Frame>`.
///
/// A fragment is a plain table, and Vide reads a table in a child slot as a
/// list of children. So a fragment inside a node adds a level that Vide takes
/// away again, and the children go straight into the node above.
///
/// This is the `noUselessFragments` rule of Biome. A fragment that stands on
/// its own is not a finding: `local rows = <><A/></>` is a table of children,
/// which is a value that Vide passes around, and an element is not.
pub fn check(_src: &str, node: &Node, out: &mut Vec<Finding>) {
    let (_, children) = super::parts(node);

    for child in children {
        let Child::Node(Node::Fragment(fragment)) = child else {
            continue;
        };

        out.push(
            Finding::new(
                NAME,
                super::range(fragment.span),
                "this fragment is inside another node, where it adds nothing",
            )
            .with_help("move its children into the node above, and remove the `<>` and the `</>`"),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{findings_named, text};
    use super::*;

    #[test]
    fn reports_a_fragment_inside_an_element() {
        let src = "return <Frame><><A/><B/></></Frame>";
        let findings = findings_named(src, NAME);

        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(text(src, &findings[0]), "<><A/><B/></>");
    }

    #[test]
    fn reports_a_fragment_inside_a_fragment() {
        assert_eq!(findings_named("return <><><A/></></>", NAME).len(), 1);
    }

    #[test]
    fn a_fragment_that_stands_on_its_own_is_not_a_finding() {
        // The table is the value here, and an element is not the same thing.
        assert!(findings_named("local rows = <><A/><B/></>", NAME).is_empty());
    }

    #[test]
    fn a_fragment_in_a_hole_is_not_a_finding() {
        // The hole holds an expression, and that expression is the table.
        let src = "return <Frame>{cond and <><A/></> or nil}</Frame>";

        assert!(findings_named(src, NAME).is_empty());
    }
}
