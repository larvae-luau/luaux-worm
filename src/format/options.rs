//! The format options of this worm, and the table that carries them.
//!
//! larvae owns the width, the indentation, and the printer, so those settings
//! are settings of larvae. What is left is the markup itself: the quotes of an
//! attribute, the place of the closing bracket of a tag, and the way that text
//! wraps. Each of those is one option here, with the same default as the rule
//! of Prettier or Biome that it comes from.
//!
//! `worm.toml` declares each one under `[fmt]`, and the user writes it in the
//! `[fmt]` table of `larvae.toml`, beside `column_width`:
//!
//! ```toml
//! [fmt]
//! column_width         = 100
//! luaux_quote_style    = "single"
//! luaux_text_wrap      = "preserve"
//! ```
//!
//! larvae fills every option that the user leaves out, checks each name
//! against its own options and every other worm, and hands the whole table to
//! `init`. So one table holds every format option of a project, and a reader
//! of `larvae.toml` learns one place and not one place per worm.

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
    /// larvae sends this table at `init`. It holds the options of larvae as
    /// well, such as `column_width`, so a name that is not an option of this
    /// worm passes without a word. larvae checks each name and fills each
    /// default before it sends the table, so there is nothing left to refuse.
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
            let value = match value {
                serde_json::Value::Bool(value) => Value::Boolean(*value),
                serde_json::Value::String(value) => Value::Word(value),
                serde_json::Value::Number(value) => match value.as_i64() {
                    Some(value) => Value::Number(value),
                    None => Value::Other,
                },
                _ => Value::Other,
            };

            options.put(key, value)?;
        }

        Ok(options)
    }

    /// Puts one value of the table in its place.
    ///
    /// A name that no option of this worm takes belongs to larvae or to an
    /// other worm, and it passes without a word. larvae checks each name
    /// against every declaration before it sends the table, so a name that is
    /// nobody's never arrives.
    fn put(&mut self, key: &str, value: Value) -> Result<(), String> {
        match key {
            "luaux_attribute_quotes" => {
                self.attribute_quotes = match word(key, value)? {
                    "double" => QuoteStyle::Double,
                    "single" => QuoteStyle::Single,
                    "preserve" => QuoteStyle::Preserve,
                    other => return Err(one_of(key, other, "double, single, preserve")),
                }
            }

            "luaux_text_wrap" => {
                self.text_wrap = match word(key, value)? {
                    "fill" => TextWrap::Fill,
                    "preserve" => TextWrap::Preserve,
                    other => return Err(one_of(key, other, "fill, preserve")),
                }
            }

            // Two settings of larvae that this worm follows for what it lays
            // out itself. They keep the names of larvae, because a project
            // states a setting one time.
            "indent_width" => self.indent_width = number(key, value)?.max(1) as usize,
            "space_inside_braces" => self.space_inside_braces = boolean(key, value)?,

            "luaux_bracket_same_line" => self.bracket_same_line = boolean(key, value)?,
            "luaux_attribute_per_line" => self.attribute_per_line = boolean(key, value)?,
            "luaux_self_closing_space" => self.self_closing_space = boolean(key, value)?,
            "luaux_blank_lines" => self.blank_lines = boolean(key, value)?,

            _ => {}
        }

        Ok(())
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

    /// One option of the `[fmt]` table, as larvae sends it.
    fn one(name: &str, value: &str) -> Result<Options, String> {
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
        let options = one("luaux_text_wrap", "\"preserve\"").expect("the options");

        assert_eq!(options.text_wrap, TextWrap::Preserve);
        assert_eq!(options.attribute_quotes, QuoteStyle::Double);
    }

    #[test]
    fn every_option_reads_its_value() {
        let options = Options::read_json(
            r#"{"luaux_attribute_quotes":"single","luaux_bracket_same_line":true,
                "luaux_attribute_per_line":true,"luaux_self_closing_space":false,
                "luaux_text_wrap":"preserve","luaux_blank_lines":false}"#,
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
            r#"{"column_width":100,"indent_width":2,"luaux_attribute_quotes":"single",
                "luaux_blank_lines":false}"#,
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
        let error = one("luaux_attribute_quotes", "\"curly\"").expect_err("an error");

        assert!(error.contains("double, single, preserve"), "{error}");
    }

    #[test]
    fn an_option_of_larvae_passes_without_a_word() {
        // `column_width` reaches the `host` spans without this worm.
        assert!(one("column_width", "100").is_ok());
        assert!(one("sort_requires", "true").is_ok());
    }

    #[test]
    fn a_value_of_the_wrong_kind_is_an_error() {
        assert!(one("luaux_bracket_same_line", "\"yes\"").is_err());
        assert!(one("luaux_text_wrap", "true").is_err());
    }
}
