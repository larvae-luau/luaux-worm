//! The Luau view of a `.luaux` file.
//!
//! `inherit_lints = true` in the manifest asks larvae to run its own lints on a
//! claimed file. Those lints read Luau, and a `.luaux` file is not Luau, so the
//! worm hands larvae a shadow of the file: the same bytes, with every markup
//! region replaced by Luau of the same byte length. Each offset then matches
//! the source, so a finding of larvae points at the exact line and column of
//! the file that the author wrote, and no lint reads generated code.
//!
//! Two rules hold for every region that this module replaces:
//!
//! - **The byte length and the newlines stay.** A shadow that moves an offset
//!   reports a finding in the wrong place.
//! - **What the region reads stays.** A region becomes a table, and each part
//!   of it that reads a value of the file keeps its place inside that table:
//!   the expression of each hole, and the name of each element that is a
//!   component. Filler that dropped them would make `unused_variable` report
//!   every value that the markup reads, and a UI file reads most of its values
//!   there.
//!
//! ```text
//! local size = UDim2.fromScale(1, 1)     local size = UDim2.fromScale(1, 1)
//! return <Frame Size={size}>          -> return {          size,
//!     <Row Name="a"/>                            {Row         },
//! </Frame>                                }
//! ```
//!
//! A table is the filler because a table is a value in every position that a
//! markup region can take, it holds any number of entries, and it fits the
//! shortest region there is: `<A/>` becomes `{  }`.

use luaux::Config;
use luaux::markup::{Attribute, AttributeValue, Child, ElementName, Node, Span};
use luaux::roblox;

use crate::scan::{self, Segment};

/// The file as Luau, or `None` when the markup does not parse.
///
/// larvae falls back to the output of `transform` when the reply carries no
/// shadow, so a file that this module cannot read still gets the lints of
/// larvae, by line.
pub fn view(src: &str, config: &Config) -> Option<String> {
    let shadow = range(src, 0, src.len(), config)?;

    debug_assert_eq!(shadow.len(), src.len(), "the shadow keeps the byte length");

    Some(shadow)
}

/// The shadow of one range of the source.
///
/// The text between two markup regions is Luau already, and it crosses byte for
/// byte. A hole holds a range of this kind as well, which is why this takes a
/// range and not the whole file.
fn range(src: &str, start: usize, end: usize, config: &Config) -> Option<String> {
    let segments = scan::segments(src, start, end).ok()?;
    let mut out = String::with_capacity(end - start);

    for segment in &segments {
        match segment {
            Segment::Luau { start, end } => out.push_str(&src[*start..*end]),

            Segment::Markup { node, start, end } => {
                let mut region = Region::new(src, *start, *end);
                region.keep_node(node, config);
                region.keep_factory(&config.create);
                out.push_str(&region.finish());
            }
        }
    }

    Some(out)
}

/// One markup region on its way to becoming a Luau table.
///
/// The region starts as a table with nothing in it: a `{`, a `}`, and spaces
/// and newlines between them. Each part that reads a value goes back in at the
/// offset that it has in the source, and a comma follows it, so the table holds
/// one entry for each of those parts.
struct Region<'a> {
    src: &'a str,
    start: usize,
    bytes: Vec<u8>,
    /// Whether an element of this region compiles to a call of the factory
    calls_the_factory: bool,
}

impl<'a> Region<'a> {
    fn new(src: &'a str, start: usize, end: usize) -> Self {
        // Every byte that reads nothing becomes a space. A newline stays where
        // it is, so no line of the file moves.
        let mut bytes: Vec<u8> = src[start..end]
            .bytes()
            .map(|byte| if byte == b'\n' { b'\n' } else { b' ' })
            .collect();

        // The shortest region is `<A/>`, so the two braces always fit.
        let last = bytes.len() - 1;
        bytes[0] = b'{';
        bytes[last] = b'}';

        Self {
            src,
            start,
            bytes,
            calls_the_factory: false,
        }
    }

    /// Puts one expression of the source back at its own offset.
    ///
    /// The comma goes in the byte after the expression. That byte is free in
    /// every case: a hole ends with `}`, and the name of an element is followed
    /// by whitespace, by `/`, or by `>`.
    fn keep(&mut self, start: usize, end: usize, text: &str) {
        let at = start - self.start;
        let after = end - self.start;

        // The last byte of the region is the closing brace of the table, and
        // nothing takes its place.
        if text.len() != after - at || after >= self.bytes.len() {
            return;
        }

        self.bytes[at..after].copy_from_slice(text.as_bytes());

        // The last byte of the region is the closing brace of the table, and a
        // comma never takes its place. An entry that ends beside it needs none.
        if after < self.bytes.len() - 1 {
            self.bytes[after] = b',';
        }
    }

    /// Keeps everything of one node that reads a value.
    fn keep_node(&mut self, node: &Node, config: &Config) {
        let (attributes, children) = match node {
            Node::Element(element) => {
                self.keep_element_name(&element.name, element.span, config);

                (element.attributes.as_slice(), element.children.as_slice())
            }

            Node::Fragment(fragment) => (&[][..], fragment.children.as_slice()),
        };

        for attribute in attributes {
            match attribute {
                Attribute::Named {
                    value: AttributeValue::Expression(_),
                    span,
                    ..
                } => {
                    let braces = scan::brace_span(self.src, *span);
                    self.keep_hole(braces, config);
                }

                Attribute::Spread { span, .. } => self.keep_hole(*span, config),

                Attribute::Named { .. } => {}
            }
        }

        for child in children {
            match child {
                Child::Node(node) => self.keep_node(node, config),

                Child::Expression { span, .. } => self.keep_hole(*span, config),

                // A comment reads nothing, and a text child is text.
                Child::Comment { .. } | Child::Text { .. } => {}
            }
        }
    }

    /// The name of an element, when that name is a value of the file.
    ///
    /// `<MyButton/>` reads the binding `MyButton`, so the shadow reads it as
    /// well. `<Frame/>` reads nothing: the name is a Roblox class, and a class
    /// is not a name in the file. An alias of a class is not one either.
    fn keep_element_name(&mut self, name: &ElementName, span: Span, config: &Config) {
        if let ElementName::Simple(simple) = name {
            let aliased = matches!(config.resolve_element(simple), Ok(Some(_)));

            if aliased || roblox::is_class(simple) {
                // `<Frame/>` compiles to `create("Frame")(...)`, so the file
                // reads the factory here, and the name of the class is not a
                // name of the file.
                self.calls_the_factory = true;

                return;
            }
        }

        // The name starts one byte after the `<` of the element. A member name
        // such as `Foo.Bar` reads `Foo`, and it is an expression as it stands.
        let written = name.as_written();
        let start = span.start + 1;

        self.keep(start, start + written.len(), &written);
    }

    /// The expression inside a hole, with any markup inside it shadowed too.
    fn keep_hole(&mut self, span: Span, config: &Config) {
        let (start, end) = scan::hole_inner(self.src, span);

        if start == end {
            return;
        }

        // A hole can hold markup of its own, and that markup is not Luau. It
        // becomes a table in the same way, and the Luau around it stays.
        let Some(inside) = range(self.src, start, end, config) else {
            return;
        };

        self.keep(start, end, &inside);
    }

    /// Writes the element factory into a free stretch of the region.
    ///
    /// An intrinsic element compiles to a call of whatever `[factory] create`
    /// names, so a file that holds one reads that name. Without this, the
    /// import at the top of every luaux file is a finding of
    /// `unused_variable`.
    ///
    /// The name goes in after the rest, and only where the region still holds
    /// spaces, so it takes the place of no other entry. A region with no room
    /// for it keeps its spaces, and an other region of the file carries it.
    fn keep_factory(&mut self, factory: &str) {
        if !self.calls_the_factory || factory.is_empty() {
            return;
        }

        let last = self.bytes.len() - 1;
        let mut spaces = 0;

        for index in 1..last {
            if self.bytes[index] != b' ' {
                spaces = 0;
                continue;
            }

            spaces += 1;

            if spaces < factory.len() {
                continue;
            }

            let at = index + 1 - factory.len();
            self.bytes[at..=index].copy_from_slice(factory.as_bytes());

            if index + 1 < last {
                self.bytes[index + 1] = b',';
            }

            return;
        }
    }

    fn finish(self) -> String {
        String::from_utf8(self.bytes).expect("the shadow holds the bytes that it wrote")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shadow(src: &str) -> String {
        view(src, &Config::default()).expect("a shadow")
    }

    /// Every shadow holds these, whatever the file is.
    fn holds_the_rules(src: &str) -> String {
        let shadow = shadow(src);

        assert_eq!(shadow.len(), src.len(), "the byte length moved:\n{shadow}");
        assert_eq!(
            shadow.match_indices('\n').collect::<Vec<_>>(),
            src.match_indices('\n').collect::<Vec<_>>(),
            "a newline moved:\n{shadow}"
        );

        let parsed = full_moon::parse_fallible(&shadow, full_moon::LuaVersion::luau());

        assert!(
            parsed.errors().is_empty(),
            "the shadow is not Luau:\n{shadow}\n{:?}",
            parsed.errors()
        );

        shadow
    }

    #[test]
    fn a_file_with_no_markup_is_the_file() {
        let src = "local x = 1 < 2\nreturn x\n";

        assert_eq!(holds_the_rules(src), src);
    }

    #[test]
    fn a_region_becomes_a_table_of_the_same_length() {
        // `create` is the factory, and `<Frame/>` compiles to a call of it.
        assert_eq!(
            holds_the_rules("local a = <Frame/>\n"),
            "local a = {create}\n"
        );
    }

    #[test]
    fn a_hole_keeps_the_value_that_it_reads() {
        // `size` is used by the file, and the shadow uses it in the same place,
        // so `unused_variable` stays quiet.
        let src = "local size = 1\nreturn <Frame Size={size}/>\n";
        let shadow = holds_the_rules(src);

        assert_eq!(shadow, "local size = 1\nreturn {create,     size, }\n");
    }

    #[test]
    fn a_file_that_builds_a_component_only_does_not_read_the_factory() {
        // `<Row/>` compiles to `Row({...})`, and it calls no factory.
        let shadow = holds_the_rules("local Row = r\nreturn <Row     />\n");

        assert!(!shadow.contains("create"), "{shadow}");
        assert!(shadow.contains("Row"), "{shadow}");
    }

    #[test]
    fn a_region_with_no_room_for_the_factory_keeps_its_spaces() {
        // `<A/>` is four bytes, and `create` needs six.
        holds_the_rules("local A = a\nreturn <A/>\n");
    }

    #[test]
    fn the_name_of_a_component_is_a_value_and_the_name_of_a_class_is_not() {
        // `Row` is a binding of the file. `Frame` is a Roblox class, and a
        // shadow that read it would report an undefined global.
        let src = "local Row = 1\nreturn <Frame><Row/></Frame>\n";
        let shadow = holds_the_rules(src);

        assert!(shadow.contains("Row,"), "{shadow}");
        assert!(!shadow.contains("Frame"), "{shadow}");
    }

    #[test]
    fn an_alias_of_a_class_is_not_a_value() {
        let config = Config::parse("[elements]\nFrame = \"Box\"\n").expect("config");
        let shadow = view("return <Box/>\n", &config).expect("a shadow");

        assert!(!shadow.contains("Box"), "{shadow}");
    }

    #[test]
    fn markup_inside_a_hole_keeps_the_luau_around_it() {
        let src =
            "local items = {}\nreturn <Frame>{items:map(function() return <Row/> end)}</Frame>\n";
        let shadow = holds_the_rules(src);

        // The nested region is a table as well, and `Row` is a binding.
        assert!(
            shadow.contains("items:map(function() return {Row,} end)"),
            "{shadow}"
        );
    }

    #[test]
    fn a_region_over_two_lines_keeps_every_line() {
        let src = "return <Frame\n\tSize={size}\n>\n\t<Row/>\n</Frame>\n";

        holds_the_rules(src);
    }

    #[test]
    fn a_fragment_and_a_text_child_hold_the_rules() {
        holds_the_rules("return <><TextLabel>Name: {name} here</TextLabel></>\n");
    }

    #[test]
    fn a_spread_and_a_string_hold_the_rules() {
        let shadow = holds_the_rules("local props = {}\nreturn <Frame {props} Name=\"a\"/>\n");

        assert!(shadow.contains("props,"), "{shadow}");
        assert!(!shadow.contains('"'), "{shadow}");
    }

    #[test]
    fn a_file_that_does_not_parse_has_no_shadow() {
        assert!(view("return <Frame>\n", &Config::default()).is_none());
    }
}
