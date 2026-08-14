//! Where the settings of the worm come from.
//!
//! Two files hold settings, and each one holds a different kind. `luaux.toml`
//! says what the compiler means: the element factory, the aliases, and the
//! build paths. `larvae.toml` says how the output looks: the width, the
//! indentation, and the level of each lint.
//!
//! | The setting | Where it lives | Who reads it |
//! |---|---|---|
//! | The factory, the aliases, the build paths | `luaux.toml` | the luaux crate |
//! | The width, the indent, the spacing | `settings.fmt` from larvae | this worm, for layout |
//! | The level of a lint | `settings.lint` from larvae | larvae, and not this worm |
//!
//! The worm reads `luaux.toml` the way the luaux command reads it, and moves
//! nothing out of it. luaux is a tool of its own: a user with no larvae project
//! keeps a working luaux, and a file that goes through both tools compiles the
//! same way in each.

use luaux::Config;

use crate::format::Options;

/// The name of the settings file of a luaux project.
const LUAUX_TOML: &str = "luaux.toml";

/// The option that `worm.toml` declares under `[options]`.
///
/// It names the `luaux.toml` to read, for a project that keeps that file
/// somewhere other than the directory larvae runs in. It is a setting about
/// where larvae looks, and not a setting of the luaux compiler, which is why it
/// belongs to larvae and the settings of the compiler do not.
const LUAUX_TOML_OPTION: &str = "luaux_toml";

/// Everything that the worm reads one time, before the first file.
#[derive(Default)]
pub struct Settings {
    /// What the luaux compiler reads: the aliases and the element factory
    pub config: Config,
    /// What the formatter of this worm reads
    pub format: Options,
}

/// The settings for this run, and the notes about them.
///
/// `config` is the `[worms.luaux.config]` table, as TOML. `fmt` is the resolved
/// `[fmt]` table of the project, as JSON.
pub fn read(config: &str, fmt: &str) -> Result<(Settings, Vec<String>), String> {
    let directory = std::env::current_dir().map_err(|error| {
        format!("luaux-worm cannot read the directory that it runs in: {error}")
    })?;

    let path = directory.join(luaux_toml(config)?);

    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        // A luaux project works without that file, and so does this worm.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(format!("{}: {error}", path.display())),
    };

    let (config, notes) = Config::parse_reporting(&text).map_err(|error| error.message)?;

    Ok((
        Settings {
            config,
            format: Options::read_json(fmt)?,
        },
        notes,
    ))
}

/// The `luaux.toml` that this project reads.
fn luaux_toml(config: &str) -> Result<String, String> {
    if config.trim().is_empty() {
        return Ok(String::from(LUAUX_TOML));
    }

    let table: toml::Table = config
        .parse()
        .map_err(|error| format!("[worms.luaux.config]: {error}"))?;

    match table.get(LUAUX_TOML_OPTION) {
        None => Ok(String::from(LUAUX_TOML)),
        Some(toml::Value::String(path)) => Ok(path.clone()),
        Some(_) => Err(format!(
            "[worms.luaux.config] {LUAUX_TOML_OPTION} takes the path of a file, in quotes"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_is_the_file_that_the_luaux_command_reads() {
        assert_eq!(luaux_toml("").expect("a path"), LUAUX_TOML);
        assert_eq!(luaux_toml("\n# nothing\n").expect("a path"), LUAUX_TOML);
    }

    #[test]
    fn a_project_names_another_luaux_toml() {
        let path = luaux_toml("luaux_toml = \"src/luaux.toml\"\n").expect("a path");

        assert_eq!(path, "src/luaux.toml");
    }

    #[test]
    fn a_path_that_is_not_a_path_is_an_error() {
        assert!(luaux_toml("luaux_toml = 3\n").is_err());
        assert!(luaux_toml("[luaux_toml\n").is_err());
    }

    #[test]
    fn the_fmt_table_of_larvae_reaches_the_layout() {
        let (settings, _) = read(
            "",
            r#"{"column_width":100,"luaux":{"attribute_quotes":"single"}}"#,
        )
        .expect("settings");

        assert_eq!(
            settings.format.attribute_quotes,
            crate::format::options::QuoteStyle::Single
        );
    }

    #[test]
    fn a_bad_value_for_an_option_is_an_error() {
        let error = read("", r#"{"luaux":{"text_wrap":"wrap"}}"#)
            .err()
            .expect("an error");

        assert!(error.contains("fill, preserve"), "{error}");
    }

    #[test]
    fn an_option_of_larvae_passes_without_a_word() {
        // larvae sends its whole `[fmt]` table, and it applies most of it to
        // the `host` spans without this worm.
        let table = r#"{"column_width":100,"indent_style":"tab","quote_style":"double"}"#;

        assert!(read("", table).is_ok());
    }

    #[test]
    fn no_settings_at_all_are_the_default_settings() {
        let (settings, notes) = read("", "").expect("settings");

        assert_eq!(settings.config.create, "create");
        assert_eq!(settings.format, Options::default());
        assert!(notes.is_empty());
    }
}
