//! The same attribute name two times on one element.

use larvae_worm::native::Finding;
use luaux::markup::{Attribute, Node};

/// The name that `worm.toml` declares for this rule.
pub const NAME: &str = "duplicate_attribute";

/// Reports the second `Text` of `<TextLabel Text="a" Text="b"/>`.
///
/// The last value wins, and neither luaux nor Vide says a word about it, so one
/// of the two lines does nothing. This is the `noDuplicateJsxProps` rule of
/// Biome, for markup that Vide builds.
///
/// The rule compares the name as the author writes it. A `{spread}` carries no
/// name until the code runs, so a spread beside a name is not a finding.
pub fn check(_src: &str, node: &Node, out: &mut Vec<Finding>) {
    let (attributes, _) = super::parts(node);
    let mut seen: Vec<&str> = Vec::new();

    for attribute in attributes {
        let Attribute::Named { name, span, .. } = attribute else {
            continue;
        };

        if seen.contains(&name.as_str()) {
            out.push(
                Finding::new(
                    NAME,
                    super::range(*span),
                    format!("this element already has an attribute named {name}"),
                )
                .with_help("the last value wins, so remove the one that does nothing"),
            );
        } else {
            seen.push(name);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{findings_named, text};
    use super::*;

    #[test]
    fn reports_the_second_attribute_of_the_same_name() {
        let src = r#"return <TextLabel Text="a" TextSize={18} Text="b"/>"#;
        let findings = findings_named(src, NAME);

        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(text(src, &findings[0]), r#"Text="b""#);
        assert!(findings[0].message.contains("Text"), "{findings:?}");
    }

    #[test]
    fn reports_each_name_that_comes_back() {
        let src = r#"return <Frame A={1} A={2} A={3}/>"#;

        assert_eq!(findings_named(src, NAME).len(), 2);
    }

    #[test]
    fn two_names_that_are_not_the_same_are_not_a_finding() {
        let src = r#"return <Frame Size={1} Position={2} {props}/>"#;

        assert!(findings_named(src, NAME).is_empty());
    }

    #[test]
    fn a_name_on_another_element_is_not_a_finding() {
        let src = r#"return <Frame Size={1}><TextLabel Size={2}/></Frame>"#;

        assert!(findings_named(src, NAME).is_empty());
    }

    #[test]
    fn reads_an_element_inside_a_hole() {
        let src = r#"return <Frame>{items:map(function() return <Row A={1} A={2}/> end)}</Frame>"#;

        assert_eq!(findings_named(src, NAME).len(), 1);
    }
}
