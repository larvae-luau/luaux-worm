//! A larvae worm for `.luaux` files.
//!
//! larvae starts this program one time and keeps it alive. Each message is a 4
//! byte little endian length, then that many bytes of JSON, in both directions,
//! over stdin and stdout. The `larvae-worm` crate owns that protocol, so this
//! file implements [`Handler`] and calls [`serve`].
//!
//! The worm answers three requests. `transform` gives larvae the Luau of a
//! `.luaux` file. `format` gives larvae a layout document for `larvae fmt`.
//! `lint` gives larvae the findings for `larvae lint`.
//!
//! | Module | What it does |
//! |---|---|
//! | [`scan`] | Finds the markup, the Luau, the comments, and the holes |
//! | [`shadow`] | Writes the Luau view that the inherited lints of larvae read |
//! | [`statements`] | Reads that view to find where a `host` span may start and stop |
//! | [`format`] | Builds the layout document |
//! | [`lints`] | Builds the findings |
//! | [`settings`] | Joins `luaux.toml` and the settings of larvae |
//! | [`report`] | Writes a message that a person reads |
//!
//! Never write to stdout. stdout is the protocol channel, and one extra byte
//! corrupts a reply. Write to stderr instead.

mod format;
mod lints;
mod report;
mod scan;
mod settings;
mod shadow;
mod statements;

use larvae_worm::native::{Format, Handler, Lint, Settings as FromLarvae, serve};
use luaux::Vide;

use settings::Settings;

/// The state of the worm.
///
/// The settings are the only state, because larvae processes files in parallel
/// and work stealing decides which worker sees which file. State that crosses
/// files makes the output depend on the schedule. The settings are safe: larvae
/// sends them one time, before the first file.
struct LuauxWorm {
    settings: Settings,
}

impl Handler for LuauxWorm {
    /// The settings, which larvae sends one time before the first file.
    ///
    /// `config` is the `[worms.luaux.config]` table of the project, as TOML
    /// text, and `from_larvae.fmt` is the resolved `[fmt]` table, as JSON.
    ///
    /// `rules` and `from_larvae.lint` name the lints and their levels. The worm
    /// needs neither: larvae drops a finding whose level is `allow` before it
    /// renders anything, so the worm reports what it finds and larvae decides
    /// how loudly to say it.
    fn init(&mut self, config: &str, _rules: &str, from_larvae: &FromLarvae) -> Result<(), String> {
        let (settings, notes) = settings::read(config, &from_larvae.fmt)?;

        for note in notes {
            eprintln!("luaux-worm: {note}");
        }

        self.settings = settings;

        Ok(())
    }

    /// Turns a `.luaux` file into Luau.
    ///
    /// The line count does not change. larvae keeps the line numbers through
    /// the pipeline, so a stack trace in Studio points at the line that the
    /// author wrote. The luaux backend emits the same number of newlines as the
    /// span that it replaces, and this function adds nothing around it.
    fn transform(&mut self, source: &str) -> Result<String, String> {
        let (output, _warnings) =
            luaux::compile::compile_configured(source, &Vide, self.settings.config.clone())
                .map_err(|error| report::compile_error(source, &error))?;

        Ok(output)
    }

    fn format(&mut self, source: &str) -> Result<Format, String> {
        format::format(source, &self.settings.format)
    }

    fn lint(&mut self, source: &str) -> Result<Lint, String> {
        lints::lint(source, &self.settings.config)
    }
}

/// `serve` runs until larvae closes the pipe. A handler that returns `Err`
/// becomes an error reply, and the worm continues to serve, because one bad
/// file must not stop a watch session.
fn main() {
    serve(LuauxWorm {
        settings: Settings::default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn worm() -> LuauxWorm {
        LuauxWorm {
            settings: Settings::default(),
        }
    }

    #[test]
    fn a_transform_keeps_the_line_count() {
        let source = "local vide = require(path)\nlocal create = vide.create\n\nreturn function()\n\treturn <Frame>\n\t\t<TextLabel Text=\"hi\"/>\n\t</Frame>\nend\n";
        let output = worm().transform(source).expect("luau");

        assert_eq!(output.lines().count(), source.lines().count(), "\n{output}");
    }

    #[test]
    fn a_bad_file_gives_a_message_with_a_position() {
        let error = worm().transform("return <Frame>\n").expect_err("an error");

        // The parser reads to the end of the file before it knows that the
        // element has no closing tag, so it reports the position that it
        // reached.
        assert_eq!(error, "line 2, column 1: unclosed element");
    }

    #[test]
    fn a_project_names_the_luaux_toml_that_it_keeps() {
        let mut worm = worm();
        worm.init(
            "luaux_toml = \"missing.toml\"\n",
            "",
            &FromLarvae::default(),
        )
        .expect("settings");

        // The file is not there, and a luaux project works without one.
        let output = worm
            .transform("local create = require(vide).create\nreturn <Frame/>\n")
            .expect("luau");

        assert!(output.contains("create"), "{output}");
    }

    #[test]
    fn the_fmt_table_of_larvae_reaches_the_layout() {
        let mut worm = worm();
        let from_larvae = FromLarvae {
            fmt: String::from(r#"{"luaux_attribute_quotes":"single"}"#),
            lint: String::new(),
        };

        worm.init("", "", &from_larvae).expect("settings");

        let format = worm
            .format("return <Frame Name=\"a\"/>\n")
            .expect("a layout");
        let document = serde_json::to_string(&format.document).expect("json");

        assert!(document.contains(r#"{"lit":"'"}"#), "{document}");
    }

    #[test]
    fn bad_settings_say_what_is_wrong_with_them() {
        let from_larvae = FromLarvae {
            fmt: String::from(r#"{"luaux_text_wrap":"wrap"}"#),
            lint: String::new(),
        };
        let error = worm().init("", "", &from_larvae).expect_err("an error");

        assert!(error.contains("fill, preserve"), "{error}");
    }

    #[test]
    fn no_settings_are_the_default_settings() {
        assert!(worm().init("", "", &FromLarvae::default()).is_ok());
    }
}
