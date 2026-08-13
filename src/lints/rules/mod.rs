//! The rules of this worm over the markup itself.
//!
//! Each rule is one file, in the same way as Biome keeps one rule per file. A
//! rule reads the syntax tree and reports a span of the file, so a finding
//! needs no repair. A rule reports a mistake that the compiler accepts: the
//! compiler asks whether the file builds, and a rule asks whether the file says
//! what the author means.
//!
//! To add a rule:
//!
//! 1. Declare the name in `[lints]` in `worm.toml`, and start it with `luaux_`.
//! 2. Write the file, with the name as a constant and a `check` function.
//! 3. Add the module here, and call it in [`findings`].
//! 4. Add the rule to the table in `README.md`.

mod comment_as_text;
mod duplicate_attribute;
mod explicit_true;
mod self_closing_element;
mod useless_fragment;

use larvae_worm::native::Finding;
use luaux::markup::{Attribute, Child, Node, Span};

use crate::scan;

/// Every finding of every rule, over every node in the file.
pub fn findings(src: &str) -> Vec<Finding> {
    let mut findings = Vec::new();

    scan::each_node(src, &mut |node| {
        comment_as_text::check(src, node, &mut findings);
        duplicate_attribute::check(src, node, &mut findings);
        explicit_true::check(src, node, &mut findings);
        self_closing_element::check(src, node, &mut findings);
        useless_fragment::check(src, node, &mut findings);
    });

    findings
}

/// The attributes and the children of a node, whichever kind it is.
///
/// A fragment has no name and no attribute, and holds children in the same way
/// as an element does.
pub(super) fn parts(node: &Node) -> (&[Attribute], &[Child]) {
    match node {
        Node::Element(element) => (element.attributes.as_slice(), element.children.as_slice()),
        Node::Fragment(fragment) => (&[], fragment.children.as_slice()),
    }
}

/// The span of the syntax tree, as larvae reads a span.
pub(super) fn range(span: Span) -> (u32, u32) {
    (span.start as u32, span.end as u32)
}

#[cfg(test)]
pub(super) mod tests {
    use larvae_worm::native::Finding;

    /// The findings of every rule over one source.
    pub fn findings(src: &str) -> Vec<Finding> {
        super::findings(src)
    }

    /// The findings under one name, which is what a test of one rule reads.
    pub fn findings_named(src: &str, name: &str) -> Vec<Finding> {
        findings(src)
            .into_iter()
            .filter(|finding| finding.lint == name)
            .collect()
    }

    /// The text under the span of a finding.
    pub fn text<'a>(src: &'a str, finding: &Finding) -> &'a str {
        &src[finding.span.0 as usize..finding.span.1 as usize]
    }
}
