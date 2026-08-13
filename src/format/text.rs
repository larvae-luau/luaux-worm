//! A text child, and the whitespace rule of luaux.
//!
//! The markup parser drops the whitespace next to a newline, because that
//! whitespace is indentation. It keeps the other whitespace, because that
//! whitespace is text. It then joins the lines that are left with one space.
//!
//! Three results follow, and this module is built on them:
//!
//! - A newline in a run is free. The parser gives back one space for it, so a
//!   part of a run can move to the next line and the text does not change.
//! - One space between two words is free in the same way. A break there becomes
//!   a newline, and the parser gives the space back.
//! - Two spaces are not free, and neither is a space at the start or the end of
//!   the run. The parser keeps the first as text and drops the second, so a
//!   break at one of them changes the text.
//!
//! So a run becomes a list of parts that a break may separate. Under
//! `luaux_text_wrap = "fill"`, which is the default, each part is a word and the run
//! fills each line with as many words as the line holds, in the same way as
//! Prettier and Biome lay out the text of a JSX element. Under
//! `luaux_text_wrap = "preserve"`, each part is a line of the author, and it stays
//! one line.

use larvae_worm::native::Doc;
use luaux::markup::Span;

use super::Layout;
use super::options::TextWrap;

/// The parts of a text run that the whitespace rule keeps.
pub(super) struct TextRun {
    /// The span of each part, in source order
    pub parts: Vec<(usize, usize)>,
    /// The run starts with whitespace that is text, and not indentation
    pub sticky_left: bool,
    /// The run ends with whitespace that is text, and not indentation
    pub sticky_right: bool,
}

impl TextRun {
    /// The run as one document, which fills the line.
    ///
    /// Each part after the first sits in a group with the space before it, and
    /// with nothing else. A group holds one part only, so the printer asks
    /// whether that part fits the line that it is on, and not whether the rest
    /// of the run fits. Thus the run fills each line to the width, and the text
    /// reads as a paragraph.
    ///
    /// A group over the rest of the run would break at the first part and then
    /// keep the tail flat, which is one word per line for a long run.
    ///
    /// `luaux_text_wrap = "preserve"` asks for the lines of the author instead. A
    /// hard break holds each of those lines, because a group would join two
    /// short lines that fit one line together, and that is a fill again.
    pub fn doc(&self, wrap: TextWrap) -> Doc {
        let mut parts = Vec::new();

        for (index, (start, end)) in self.parts.iter().enumerate() {
            let part = Doc::src(*start as u32, *end as u32);

            parts.push(match (index, wrap) {
                (0, _) => part,
                (_, TextWrap::Fill) => Doc::group(Doc::concat([Doc::Line, part])),
                (_, TextWrap::Preserve) => Doc::concat([Doc::Hard, part]),
            });
        }

        Doc::concat(parts)
    }
}

/// Reads one text run.
pub(super) fn run(layout: &Layout, span: Span) -> TextRun {
    let src = layout.src;
    let raw = &src[span.start..span.end];
    let lines: Vec<&str> = raw.split('\n').collect();
    let last = lines.len() - 1;

    let mut run = TextRun {
        parts: Vec::new(),
        sticky_left: false,
        sticky_right: false,
    };

    let mut line_start = span.start;

    for (index, line) in lines.iter().enumerate() {
        let line_end = line_start + line.len();
        let mut start = line_start;
        let mut end = line_end;

        // A line after a newline starts with indentation, and a line before a
        // newline ends with indentation.
        if index > 0 {
            start += line.len() - line.trim_start().len();
        }
        if index < last {
            end -= line.len() - line.trim_end().len();
        }

        if start < end {
            if index == 0 && line.starts_with(char::is_whitespace) {
                run.sticky_left = true;
            }
            if index == last && line.ends_with(char::is_whitespace) {
                run.sticky_right = true;
            }

            // A line of the author is one part under `preserve`, and the words
            // of that line are the parts under `fill`.
            match layout.options.text_wrap {
                TextWrap::Fill => words(src, start, end, &mut run.parts),
                TextWrap::Preserve => run.parts.push((start, end)),
            }
        }

        line_start = line_end + 1;
    }

    run
}

/// Cuts one line of a run at each space that a break may replace.
///
/// The space must have a character that is not whitespace on each side. A space
/// beside another space is text that a break would lose, and a space at the
/// edge of the run is text as well, because the parser keeps it.
fn words(src: &str, start: usize, end: usize, out: &mut Vec<(usize, usize)>) {
    let bytes = &src.as_bytes()[start..end];
    let mut word = start;

    for index in 1..bytes.len().saturating_sub(1) {
        let alone = bytes[index] == b' '
            && !bytes[index - 1].is_ascii_whitespace()
            && !bytes[index + 1].is_ascii_whitespace();

        if alone {
            out.push((word, start + index));
            word = start + index + 1;
        }
    }

    out.push((word, end));
}

#[cfg(test)]
mod tests {
    use super::super::Options;
    use super::super::tests::{json, json_under};
    use super::*;

    fn parts(src: &str, span: Span) -> (Vec<&str>, bool, bool) {
        let options = Options::default();
        let run = run(
            &Layout {
                src,
                options: &options,
            },
            span,
        );

        (
            run.parts
                .iter()
                .map(|(start, end)| &src[*start..*end])
                .collect(),
            run.sticky_left,
            run.sticky_right,
        )
    }

    #[test]
    fn indentation_goes_away_and_a_space_at_the_end_stays() {
        // `<L>\n  Name: {x}` — the first line is indentation only, and the
        // space after the colon is text.
        let (parts, left, right) = parts("<L>\n  Name: {x}", Span::new(3, 12));

        // The space at the end stays inside the part, because a break there
        // would put it next to a newline, and the parser drops that.
        assert_eq!(parts, ["Name: "]);
        assert!(!left);
        assert!(right);
    }

    #[test]
    fn a_run_of_spaces_between_two_elements_is_text() {
        // `<A/> <B/>` holds a space child, and `<A/>\n<B/>` does not.
        let (parts, left, right) = parts("<A/> <B/>", Span::new(4, 5));

        assert_eq!(parts, [" "]);
        assert!(left);
        assert!(right);
    }

    #[test]
    fn each_word_is_a_part() {
        let src = "<L>one two three</L>";
        let (parts, ..) = parts(src, Span::new(3, 16));

        assert_eq!(parts, ["one", "two", "three"]);
    }

    #[test]
    fn two_spaces_hold_a_line_together() {
        // The parser keeps both spaces, and a break gives back one space only.
        let src = "<L>one  two</L>";
        let (parts, ..) = parts(src, Span::new(3, 11));

        assert_eq!(parts, ["one  two"]);
    }

    #[test]
    fn a_line_of_a_run_joins_the_next_line_with_one_space() {
        // `line` is a space when flat and a newline when broken. The parser
        // gives back one space for the newline, so the text is the same.
        let doc = json("<Label>\n\tHi\n\tHello\n</Label>");

        assert!(
            doc.contains(r#"{"src":[9,11]},{"group":{"concat":["line",{"src":[13,18]}]}}"#),
            "{doc}"
        );
    }

    #[test]
    fn text_wrap_preserve_keeps_the_lines_of_the_author() {
        let options = Options {
            text_wrap: TextWrap::Preserve,
            ..Options::default()
        };
        let doc = json_under("<Label>\n\tone two\n\tthree\n</Label>", &options);

        // One part for each line of the source, and a hard break between them.
        assert!(
            doc.contains(r#"{"src":[9,16]},{"concat":["hard",{"src":[18,23]}]}"#),
            "{doc}"
        );
    }

    #[test]
    fn a_long_run_fills_the_line_instead_of_breaking_at_every_part() {
        // Each part after the first sits in a group of its own, so the printer
        // fills the line and does not put one word on each line.
        let doc = json("<Label>one two three four</Label>");

        assert_eq!(
            doc.matches(r#"{"group":{"concat":["line""#).count(),
            3,
            "{doc}"
        );
    }
}
