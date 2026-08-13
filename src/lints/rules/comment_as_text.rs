//! Text that reads as a comment.

use larvae_worm::native::Finding;
use luaux::markup::{Child, Node};

/// The name that `worm.toml` declares for this rule.
pub const NAME: &str = "luaux_comment_as_text";

/// Reports the `-- a note` of `<TextLabel>-- a note</TextLabel>`.
///
/// `--` starts a comment in Luau, and `//` starts one in other languages.
/// Between two tags neither one does: the text goes into the `Text` property,
/// and the player reads it on the screen.
///
/// This is the `noCommentText` rule of Biome. luaux writes a comment in markup
/// as `<!-- ... -->` or as `{--[[ ... ]]}`.
pub fn check(_src: &str, node: &Node, out: &mut Vec<Finding>) {
    let (_, children) = super::parts(node);

    for child in children {
        let Child::Text { text, span } = child else {
            continue;
        };

        if !text.starts_with("--") && !text.starts_with("//") {
            continue;
        }

        out.push(
            Finding::new(
                NAME,
                super::range(*span),
                "this text is not a comment, and the player reads it on the screen",
            )
            .with_help("write <!-- a note --> or {--[[ a note ]]} for a comment"),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::findings_named;
    use super::*;

    #[test]
    fn reports_text_that_starts_with_two_hyphens() {
        let src = "return <TextLabel>-- a note</TextLabel>";

        assert_eq!(findings_named(src, NAME).len(), 1);
    }

    #[test]
    fn reports_text_that_starts_with_two_slashes() {
        let src = "return <TextLabel>// a note</TextLabel>";

        assert_eq!(findings_named(src, NAME).len(), 1);
    }

    #[test]
    fn a_comment_of_luaux_is_not_a_finding() {
        assert!(findings_named("return <Frame><!-- a note --></Frame>", NAME).is_empty());
        assert!(findings_named("return <Frame>{--[[ a note ]]}</Frame>", NAME).is_empty());
    }

    #[test]
    fn ordinary_text_is_not_a_finding() {
        assert!(findings_named("return <TextLabel>a note</TextLabel>", NAME).is_empty());
        assert!(findings_named("return <TextLabel>5 - 3</TextLabel>", NAME).is_empty());
    }
}
