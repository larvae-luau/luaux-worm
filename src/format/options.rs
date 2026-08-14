//! The format options of this worm, and the table that carries them.
//!
//! larvae owns the width, the indentation, and the printer, so those settings
//! are settings of larvae. What is left is the markup itself: the quotes of an
//! attribute, the place of the closing bracket of a tag, and the way that text
//! wraps. Each of those is one option here, with the same default as the rule
//! of Prettier or Biome that it comes from.
//!
//! `worm.toml` declares each one under `[fmt]`, with a bare name. A project
//! writes them together in the table of this worm, beside the options of
//! larvae:
//!
//! ```toml
//! [fmt]
//! column_width = 100
//!
//! [fmt.luaux]
//! attribute_quotes = "single"
//! text_wrap        = "preserve"
//! ```
//!
//! larvae fills every option that the user leaves out, checks each name
//! against its own options and every other worm, and hands the whole table to
//! `init`. So one table holds every format option of a project, and a reader
//! of `larvae.toml` learns one place and not one place per worm.
//!
//! A lint and a format option read the same way: a bare name in `worm.toml`,
//! and a table under the key of this worm in the file of the project.

/// Every option of the layout, with the default of each one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// `luaux_attribute_quotes`
    pub attribute_quotes: QuoteStyle,
    /// `luaux_bracket_same_line`
    pub bracket_same_line: bool,
    /// `luaux_attribute_per_line`
    pub attribute_per_line: bool,
    /// `luaux_self_closing_space`
    pub self_closing_space: bool,
    /// `luaux_text_wrap`
    pub text_wrap: TextWrap,
    /// `luaux_blank_lines`
    pub blank_lines: bool,

    /// `indent_width` of larvae, which says how many spaces make one level
    pub indent_width: usize,
    /// `space_inside_braces` of larvae, which the braces of a hole follow
    pub space_inside_braces: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            attribute_quotes: QuoteStyle::Double,
            bracket_same_line: false,
            attribute_per_line: false,
            self_closing_space: true,
            text_wrap: TextWrap::Fill,
            blank_lines: true,
            // The defaults of larvae, for a run that sends no table at all.
            indent_width: 4,
            space_inside_braces: true,
        }
    }
}

/// `luaux_attribute_quotes`
///
/// larvae has `quote_style`, and it governs Luau strings. The value of an
/// attribute is not a Luau string: luaux keeps the quotes that the author
/// wrote, and an attribute string and a text child do not decode the same
/// escapes. So this worm has an option of its own, and a user who sets
/// `quote_style` still means their Luau.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuoteStyle {
    /// `Name="a"`, which is the default of Biome for JSX as well
    Double,
    /// `Name='a'`
    Single,
    /// The quotes that the author wrote
    Preserve,
}

impl QuoteStyle {
    /// The quote that this style asks for, or `None` for the one in the source.
    pub fn quote(self) -> Option<u8> {
        match self {
            Self::Double => Some(b'"'),
            Self::Single => Some(b'\''),
            Self::Preserve => None,
        }
    }
}

/// `luaux_text_wrap`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextWrap {
    /// Fill each line with as many words as it holds, as a paragraph does
    Fill,
    /// Break the text where the author broke it
    Preserve,
}

/// One value of the `[fmt]` table, in the two shapes that carry it.
///
/// An option holds one scalar, so these are the only shapes that reach a worm.
enum Value<'a> {
    Boolean(bool),
    Word(&'a str),
    Number(i64),
    /// A value of larvae, such as a number, that no option of this worm reads
    Other,
}

impl Options {
    /// Reads the `[fmt]` table that larvae resolves, as JSON.
    ///
    /// larvae sends this table at `init`. The options of this worm sit in the
    /// table under its own key, and the options of larvae sit beside it, so a
    /// name is read in one of two places and never in both.
    ///
    /// larvae checks each name against every declaration and fills each
    /// default before it sends the table, so a name that nobody declares never
    /// arrives here.
    pub fn read_json(text: &str) -> Result<Self, String> {
        if text.trim().is_empty() {
            return Ok(Self::default());
        }

        let table: serde_json::Value = serde_json::from_str(text)
            .map_err(|error| format!("[fmt] is not the JSON that larvae sends: {error}"))?;

        let Some(table) = table.as_object() else {
            return Err(String::from("[fmt] is a table"));
        };

        let mut options = Self::default();

        for (key, value) in table {
            match (key.as_str(), value.as_object()) {
                // The table of this worm, where each name is bare.
                (WORM, Some(own)) => {
                    for (key, value) in own {
                        options.put_own(key, scalar(value))?;
                    }
                }

                // A setting of larvae, which this worm follows for what it
                // lays out itself.
                _ => options.put_of_larvae(key, scalar(value))?,
            }
        }

        Ok(options)
    }

    /// One option of this worm, under its own bare name.
    ///
    /// larvae refuses a name in `[fmt.luaux]` that the manifest does not
    /// declare, so a name that reaches this and matches nothing is a
    /// declaration with no field behind it. A test reads the manifest and
    /// holds each name to a field, which is where that mistake is caught.
    fn put_own(&mut self, key: &str, value: Value) -> Result<(), String> {
        match key {
            "attribute_quotes" => {
                self.attribute_quotes = match word(key, value)? {
                    "double" => QuoteStyle::Double,
                    "single" => QuoteStyle::Single,
                    "preserve" => QuoteStyle::Preserve,
                    other => return Err(one_of(key, other, "double, single, preserve")),
                }
            }

            "text_wrap" => {
                self.text_wrap = match word(key, value)? {
                    "fill" => TextWrap::Fill,
                    "preserve" => TextWrap::Preserve,
                    other => return Err(one_of(key, other, "fill, preserve")),
                }
            }

            "bracket_same_line" => self.bracket_same_line = boolean(key, value)?,
            "attribute_per_line" => self.attribute_per_line = boolean(key, value)?,
            "self_closing_space" => self.self_closing_space = boolean(key, value)?,
            "blank_lines" => self.blank_lines = boolean(key, value)?,

            _ => {}
        }

        Ok(())
    }

    /// One setting of larvae that this worm follows.
    ///
    /// larvae applies every one of them to a `host` span already. These two
    /// reach the constructs that this worm lays out itself, and they keep the
    /// names of larvae, because a project states a setting one time.
    fn put_of_larvae(&mut self, key: &str, value: Value) -> Result<(), String> {
        match key {
            "indent_width" => self.indent_width = number(key, value)?.max(1) as usize,
            "space_inside_braces" => self.space_inside_braces = boolean(key, value)?,

            _ => {}
        }

        Ok(())
    }
}

/// The key of this worm in `[worms]`, which is the key of its table in `[fmt]`.
const WORM: &str = "luaux";

fn scalar(value: &serde_json::Value) -> Value<'_> {
    match value {
        serde_json::Value::Bool(value) => Value::Boolean(*value),
        serde_json::Value::String(value) => Value::Word(value),
        serde_json::Value::Number(value) => match value.as_i64() {
            Some(value) => Value::Number(value),
            None => Value::Other,
        },
        _ => Value::Other,
    }
}

fn word<'a>(key: &str, value: Value<'a>) -> Result<&'a str, String> {
    match value {
        Value::Word(word) => Ok(word),
        _ => Err(format!("[fmt] {key} takes a word in quotes")),
    }
}

fn number(key: &str, value: Value) -> Result<i64, String> {
    match value {
        Value::Number(number) => Ok(number),
        _ => Err(format!("[fmt] {key} takes a whole number")),
    }
}

fn boolean(key: &str, value: Value) -> Result<bool, String> {
    match value {
        Value::Boolean(value) => Ok(value),
        _ => Err(format!("[fmt] {key} takes true or false")),
    }
}

fn one_of(key: &str, given: &str, allowed: &str) -> String {
    format!("[fmt] {key} = \"{given}\" is not one of {allowed}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One option of this worm, in the table that larvae sends.
    fn one(name: &str, value: &str) -> Result<Options, String> {
        Options::read_json(&format!("{{\"luaux\":{{\"{name}\":{value}}}}}"))
    }

    /// One option of larvae, which sits beside the table of this worm.
    fn one_of_larvae(name: &str, value: &str) -> Result<Options, String> {
        Options::read_json(&format!("{{\"{name}\":{value}}}"))
    }

    /// Each option that `worm.toml` declares reaches a field of `Options`.
    ///
    /// larvae fills every declared option before it sends the table, so a name
    /// in the manifest that no field reads is a setting that goes quiet.
    #[test]
    fn every_option_of_the_manifest_reaches_the_layout() {
        let manifest: toml::Table = include_str!("../../worm.toml").parse().expect("worm.toml");

        let declared = manifest["fmt"].as_table().expect("a table of options");
        let default = Options::default();

        for (name, entry) in declared {
            let entry = entry.as_table().expect("an option");

            // A value that no field reads leaves the defaults as they are.
            let read = one(name, &other_than(&entry["default"])).expect("the options");

            assert_ne!(read, default, "[fmt] {name} reaches nothing");
        }
    }

    /// A value of the same kind, and never the value that the option has.
    fn other_than(value: &toml::Value) -> String {
        match value {
            toml::Value::Boolean(value) => (!value).to_string(),
            toml::Value::String(word) => format!(
                "\"{}\"",
                match word.as_str() {
                    "double" => "single",
                    "fill" => "preserve",
                    other => panic!("the test knows no other value for {other}"),
                }
            ),
            other => panic!("an option holds one scalar, and not {other}"),
        }
    }

    #[test]
    fn the_defaults_follow_prettier_and_biome() {
        let options = Options::default();

        assert_eq!(options.attribute_quotes, QuoteStyle::Double);
        assert!(!options.bracket_same_line);
        assert!(!options.attribute_per_line);
        assert_eq!(options.text_wrap, TextWrap::Fill);
    }

    #[test]
    fn a_project_sets_one_option_and_keeps_the_rest() {
        let options = one("text_wrap", "\"preserve\"").expect("the options");

        assert_eq!(options.text_wrap, TextWrap::Preserve);
        assert_eq!(options.attribute_quotes, QuoteStyle::Double);
    }

    #[test]
    fn every_option_reads_its_value() {
        let options = Options::read_json(
            r#"{"luaux":{"attribute_quotes":"single","bracket_same_line":true,
                "attribute_per_line":true,"self_closing_space":false,
                "text_wrap":"preserve","blank_lines":false}}"#,
        )
        .expect("the options");

        assert_eq!(
            options,
            Options {
                attribute_quotes: QuoteStyle::Single,
                bracket_same_line: true,
                attribute_per_line: true,
                self_closing_space: false,
                text_wrap: TextWrap::Preserve,
                blank_lines: false,
                ..Options::default()
            }
        );
    }

    #[test]
    fn the_json_of_larvae_reads_the_same_way() {
        // The table that `settings.fmt` carries holds the options of larvae as
        // well, and this worm reads its own and steps over the rest.
        let options = Options::read_json(
            r#"{"column_width":100,"indent_width":2,
                "luaux":{"attribute_quotes":"single","blank_lines":false}}"#,
        )
        .expect("the options");

        assert_eq!(options.attribute_quotes, QuoteStyle::Single);
        assert!(!options.blank_lines);
        assert!(options.self_closing_space);
        // A setting of larvae reaches the worm under the name of larvae.
        assert_eq!(options.indent_width, 2);
    }

    #[test]
    fn no_table_at_all_is_the_default_table() {
        assert_eq!(
            Options::read_json("").expect("the options"),
            Options::default()
        );
        assert_eq!(
            Options::read_json("{}").expect("the options"),
            Options::default()
        );
    }

    #[test]
    fn a_value_that_no_option_takes_is_an_error() {
        let error = one("attribute_quotes", "\"curly\"").expect_err("an error");

        assert!(error.contains("double, single, preserve"), "{error}");
    }

    #[test]
    fn an_option_of_larvae_passes_without_a_word() {
        // `column_width` reaches the `host` spans without this worm.
        assert!(one_of_larvae("column_width", "100").is_ok());
        assert!(one_of_larvae("sort_requires", "true").is_ok());
    }

    #[test]
    fn a_setting_of_larvae_reaches_what_this_worm_lays_out() {
        let options = one_of_larvae("indent_width", "2").expect("the options");

        assert_eq!(options.indent_width, 2);

        let tight = one_of_larvae("space_inside_braces", "false").expect("the options");

        assert!(!tight.space_inside_braces);
    }

    #[test]
    fn a_value_of_the_wrong_kind_is_an_error() {
        assert!(one("bracket_same_line", "\"yes\"").is_err());
        assert!(one("text_wrap", "true").is_err());
    }
}
