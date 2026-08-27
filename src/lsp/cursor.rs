//! Where the cursor sits in a `.luaux` file.
//!
//! Hover and completion ask the same question and want different halves of
//! the answer, so one reading serves both: hover takes the whole name under
//! the cursor, completion takes the part before it.
//!
//! The reading is textual, and that is the point. A completion arrives while
//! the author is in the middle of typing `<Fra`, which no markup parser
//! accepts, so a reading that needs a tree answers nothing at exactly the
//! moment the author asked. The scan walks back from the cursor over the tag
//! it is in, and a tag is short.

/// The furthest back the scan looks for the `<` of the tag.
///
/// A tag header is a line or two. The bound stops the scan from walking a
/// whole file of ordinary Luau on a keystroke, and nothing legitimate is
/// this long.
const REACH: usize = 2000;

/// Keywords that say the `<` behind the cursor was a comparison.
const KEYWORDS: &[&str] = &[
    "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "if", "in", "local",
    "nil", "not", "or", "repeat", "return", "then", "true", "until", "while",
];

/// Keywords a `<` may follow and still open markup. Each one cannot end an
/// expression, so nothing is there for a less-than to compare.
const OPENING: &[&str] = &[
    "and", "do", "else", "elseif", "in", "not", "or", "repeat", "return", "then", "while",
];

/// Where the cursor sits.
#[derive(Debug, PartialEq, Eq)]
pub enum Spot {
    /// On the name of a tag, `<Fra|me`
    Tag {
        /// The whole name, for hover
        name: String,
        /// The part before the cursor, for completion
        prefix: String,
        /// True for `</Frame>`, which names an element and opens none
        closing: bool,
    },
    /// On an attribute name in an opening tag, `<Frame Si|ze={x}`
    Attribute {
        /// The tag this attribute is on, as written
        tag: String,
        /// The whole attribute name, for hover
        name: String,
        /// The part before the cursor, for completion
        prefix: String,
        /// The attribute names already on this tag, so completion drops them
        taken: Vec<String>,
    },
    /// Plain Luau, a hole, an attribute value, or text between tags
    Elsewhere,
}

/// Reads the cursor's position in the source.
pub fn spot(text: &str, offset: usize) -> Spot {
    let offset = offset.min(text.len());

    if !text.is_char_boundary(offset) {
        return Spot::Elsewhere;
    }

    let head = &text[..offset];

    let Some(open) = open_tag(head) else {
        return Spot::Elsewhere;
    };

    if !opens_markup(&head[..open]) {
        return Spot::Elsewhere;
    }

    let head = &head[open + 1..];
    let (closing, head) = match head.strip_prefix('/') {
        Some(rest) => (true, rest),
        None => (false, head),
    };

    /*
    A tag name starts right at the `<`. A space there means the `<` was a
    less-than, and so does a digit, because no name begins with one.
    */
    if !head.is_empty() && !head.starts_with(is_name_start) {
        return Spot::Elsewhere;
    }

    let name: String = head.chars().take_while(|c| is_tag_name(*c)).collect();

    // Still inside the name: the rest of it is what follows the cursor.
    if name.len() == head.len() {
        return Spot::Tag {
            name: name.clone() + &tail(text, offset, is_tag_name),
            prefix: name,
            closing,
        };
    }

    // A closing tag holds a name and nothing else.
    if closing {
        return Spot::Elsewhere;
    }

    let Some((prefix, taken)) = attributes(&head[name.len()..]) else {
        return Spot::Elsewhere;
    };

    Spot::Attribute {
        tag: name,
        name: prefix.clone() + &tail(text, offset, is_name),
        prefix,
        taken,
    }
}

/*
Whether a `<` here opens markup, or compares two numbers.

luaux answers this with the token before the `<`: markup opens where an
expression cannot already have ended. The reading here is the same rule over
characters, because a completion runs on a keystroke and a file that is
half typed does not tokenize.
*/
fn opens_markup(before: &str) -> bool {
    let before = before.trim_end();

    let Some(last) = before.chars().last() else {
        return true;
    };

    // `a < b`, `f(x) < y`, `"s" < t`, `t[1] < 2`: each one ends an expression.
    if last.is_ascii_digit() || matches!(last, ')' | ']' | '"' | '\'') {
        return false;
    }

    if !is_name(last) {
        return true;
    }

    let word: String = before
        .chars()
        .rev()
        .take_while(|c| is_name(*c))
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();

    // A keyword that cannot end an expression leaves the next `<` free to
    // open markup. `end`, `true`, `false` and `nil` can end one, so they
    // are not here, and neither is an ordinary name.
    OPENING.contains(&word.as_str())
}

/// The name characters that follow the cursor, which hover needs and
/// completion does not.
fn tail(text: &str, offset: usize, name: fn(char) -> bool) -> String {
    text[offset..].chars().take_while(|c| name(*c)).collect()
}

fn is_name_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_name(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// `<Foo.Bar>` is one name, and always a component.
fn is_tag_name(c: char) -> bool {
    is_name(c) || c == '.'
}

/*
The `<` of the tag the cursor is in, walking back from it.

The walk steps over a finished hole and a finished string, so a cursor after
`Size={UDim2.fromScale(1, 1)}` still finds the tag it belongs to. It stops at
a `>`, because the last tag closed, and at an unfinished `{` or quote,
because the cursor is inside a value and the value is Luau.
*/
fn open_tag(head: &str) -> Option<usize> {
    let bytes = head.as_bytes();
    let floor = bytes.len().saturating_sub(REACH);
    let mut at = bytes.len();

    while at > floor {
        at -= 1;

        match bytes[at] {
            b'<' => return Some(at),

            // The tag before the cursor is finished, so the cursor is not
            // in a tag: it is in the text or the children of an element.
            b'>' => return None,

            // An unfinished hole. The cursor is inside it, and what is
            // inside a hole is Luau.
            b'{' => return None,

            b'}' => at = matching(bytes, at, floor, b'{')?,

            b'"' | b'\'' => at = matching(bytes, at, floor, bytes[at])?,

            _ => {}
        }
    }

    None
}

/*
The index of the byte that opens what `at` closes.

Braces nest and quotes do not, so a quote takes the next one and a brace
counts. An opener that the reach does not hold means the cursor is inside
the hole or the string, and the caller stops.
*/
fn matching(bytes: &[u8], at: usize, floor: usize, open: u8) -> Option<usize> {
    let close = bytes[at];
    let mut depth = 1usize;
    let mut index = at;

    while index > floor {
        index -= 1;

        if bytes[index] == open {
            depth -= 1;

            if depth == 0 {
                return Some(index);
            }
        } else if open != close && bytes[index] == close {
            depth += 1;
        }
    }

    None
}

/*
The attribute names already written, and the one the cursor is on.

`rest` is what follows the tag name, up to the cursor. Reading it is the
whole check that this is an attribute position: a chunk that holds an `=`
with the cursor past it is a value, and a chunk that is a keyword says the
`<` was a comparison after all.
*/
fn attributes(rest: &str) -> Option<(String, Vec<String>)> {
    let mut taken = Vec::new();
    let mut current = String::new();

    // Past an `=`, or inside a `{}` spread: the chunk is a value and not a name.
    let mut value = false;
    let mut depth = 0usize;
    let mut quote: Option<char> = None;

    for c in rest.chars() {
        // Inside a string, only the quote that opened it means anything.
        if let Some(open) = quote {
            if c == open {
                quote = None;
            }

            continue;
        }

        // Inside a hole, and a hole holds Luau, which has holes of its own.
        if depth > 0 {
            match c {
                '{' => depth += 1,
                '}' => depth -= 1,
                '"' | '\'' => quote = Some(c),
                _ => {}
            }

            continue;
        }

        match c {
            // A value, or a `{props}` spread. Both end at the whitespace
            // after them, and neither one holds a name to complete.
            '{' => {
                depth = 1;
                value = true;
            }

            '"' | '\'' => {
                quote = Some(c);
                value = true;
            }

            '=' => {
                if !current.is_empty() {
                    taken.push(std::mem::take(&mut current));
                }

                value = true;
            }

            c if c.is_whitespace() => {
                if !current.is_empty() {
                    if KEYWORDS.contains(&current.as_str()) {
                        return None;
                    }

                    taken.push(std::mem::take(&mut current));
                }

                value = false;
            }

            // A name, unless a value already started here.
            c if is_name(c) || c == '.' => {
                if value {
                    continue;
                }

                current.push(c);
            }

            // Anything else is not a tag header, so the `<` was not a tag.
            _ => return None,
        }
    }

    // The cursor sits in a value, not on a name.
    if value || depth > 0 || quote.is_some() {
        return None;
    }

    if KEYWORDS.contains(&current.as_str()) {
        return None;
    }

    Some((current, taken))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The source with `|` taken out, and the offset it stood at.
    fn at(marked: &str) -> (String, usize) {
        let offset = marked.find('|').expect("a cursor");

        (marked.replace('|', ""), offset)
    }

    fn spot_at(marked: &str) -> Spot {
        let (text, offset) = at(marked);

        spot(&text, offset)
    }

    #[test]
    fn the_cursor_in_a_tag_name_reads_the_whole_name_and_the_prefix() {
        assert_eq!(
            spot_at("return (<Fra|me Size={x}>)"),
            Spot::Tag {
                name: String::from("Frame"),
                prefix: String::from("Fra"),
                closing: false,
            }
        );
    }

    #[test]
    fn a_name_that_is_not_typed_yet_is_still_a_tag() {
        assert_eq!(
            spot_at("return (<|)"),
            Spot::Tag {
                name: String::new(),
                prefix: String::new(),
                closing: false,
            }
        );
    }

    #[test]
    fn a_closing_tag_names_its_element() {
        assert_eq!(
            spot_at("</Fra|me>"),
            Spot::Tag {
                name: String::from("Frame"),
                prefix: String::from("Fra"),
                closing: true,
            }
        );
    }

    #[test]
    fn the_cursor_after_the_name_is_an_attribute_position() {
        let spot = spot_at("<Frame |>");

        assert_eq!(
            spot,
            Spot::Attribute {
                tag: String::from("Frame"),
                name: String::new(),
                prefix: String::new(),
                taken: Vec::new(),
            }
        );
    }

    #[test]
    fn a_finished_hole_does_not_hide_the_tag() {
        let spot = spot_at("<Frame Size={UDim2.fromScale(1, 1)} Ba|>");

        assert_eq!(
            spot,
            Spot::Attribute {
                tag: String::from("Frame"),
                name: String::from("Ba"),
                prefix: String::from("Ba"),
                taken: vec![String::from("Size")],
            }
        );
    }

    #[test]
    fn a_finished_string_does_not_hide_the_tag_either() {
        let spot = spot_at("<TextLabel Text=\"hello there\" Text|>");

        assert!(
            matches!(&spot, Spot::Attribute { tag, taken, .. }
                if tag == "TextLabel" && taken == &[String::from("Text")]),
            "{spot:?}"
        );
    }

    #[test]
    fn the_cursor_inside_a_hole_is_luau() {
        assert_eq!(spot_at("<Frame Size={UDim2.fro|}>"), Spot::Elsewhere);
    }

    #[test]
    fn the_cursor_inside_a_string_is_not_a_name() {
        assert_eq!(spot_at("<TextLabel Text=\"hel|\">"), Spot::Elsewhere);
    }

    #[test]
    fn the_cursor_between_children_is_not_a_tag() {
        assert_eq!(spot_at("<Frame>hello |</Frame>"), Spot::Elsewhere);
    }

    /// The scan walks back over Luau, so a less-than must not read as a tag.
    #[test]
    fn a_comparison_is_not_a_tag() {
        assert_eq!(spot_at("if a < b the|n"), Spot::Elsewhere);
        assert_eq!(spot_at("if a <b the|n"), Spot::Elsewhere);
        assert_eq!(spot_at("local n = count < 10|"), Spot::Elsewhere);
    }

    #[test]
    fn a_spread_is_not_an_attribute_name() {
        let spot = spot_at("<Frame {props} Siz|>");

        assert!(
            matches!(&spot, Spot::Attribute { prefix, .. } if prefix == "Siz"),
            "{spot:?}"
        );
    }
}
