//! An attribute that says `={true}`.

use larvae_worm::native::Finding;
use luaux::markup::{Attribute, AttributeValue, Node};

/// The name that `worm.toml` declares for this rule.
pub const NAME: &str = "luaux_explicit_true_attribute";

/// Reports the `={true}` of `<Frame Visible={true}/>`.
///
/// The name on its own means true in luaux, in the same way as it does in JSX.
/// The two forms build the same table, so the shorter one is the one to read.
///
/// This is the `noImplicitBoolean` rule of Biome, the other way around: luaux
/// has the shorthand, and this worm asks for it.
pub fn check(_src: &str, node: &Node, out: &mut Vec<Finding>) {
    let (attributes, _) = super::parts(node);

    for attribute in attributes {
        let Attribute::Named {
            name,
            value: AttributeValue::Expression(expression),
            span,
        } = attribute
        else {
            continue;
        };

        if expression != "true" {
            continue;
        }

        out.push(
            Finding::new(
                NAME,
                super::range(*span),
                format!("{name}={{true}} and {name} are the same thing"),
            )
            .with_help(format!("write {name} on its own")),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{findings_named, text};
    use super::*;

    #[test]
    fn reports_an_attribute_that_says_true() {
        let src = "return <Frame Visible={true}/>";
        let findings = findings_named(src, NAME);

        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(text(src, &findings[0]), "Visible={true}");
    }

    #[test]
    fn the_shorthand_is_not_a_finding() {
        assert!(findings_named("return <Frame Visible/>", NAME).is_empty());
    }

    #[test]
    fn a_value_that_is_not_the_word_true_is_not_a_finding() {
        assert!(findings_named("return <Frame Visible={false}/>", NAME).is_empty());
        assert!(findings_named("return <Frame Visible={shown}/>", NAME).is_empty());
        assert!(findings_named("return <Frame Visible={truely}/>", NAME).is_empty());
        // A source is a function, and Vide calls it. It is not the word `true`.
        assert!(findings_named("return <Frame Visible={isOpen}/>", NAME).is_empty());
    }
}
