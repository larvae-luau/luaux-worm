//! Where a require spec points, when it points at a `.luaux` file.
//!
//! The rule the owner of luaux set: a require of a Luau file stays larvae's,
//! untouched. A require of a `.luaux` file is this worm's, from Luau and from
//! luaux files alike, so the analyzer reads the lowering and a component
//! carries its real type across the boundary.
//!
//! Almost nobody writes the extension. `require("./Card")` is the spelling in
//! every codebase, so the extension is what this module adds, in the order
//! Luau itself searches: the file, then `init` inside the directory. A spec
//! that a Luau file answers is passed, extension or not, because larvae owns
//! that one and two owners of one spec is a race.

use std::path::{Path, PathBuf};

/// The `.luaux` file this spec names, or nothing when the spec is not this
/// worm's to answer.
pub fn resolve(from: &str, spec: &str) -> Option<String> {
    let file = Path::new(from);
    let base = file.parent()?;

    // Written with the extension, the spec says which file it means.
    if spec.ends_with(".luau") {
        return None;
    }

    let target = target(base, spec)?;

    if spec.ends_with(".luaux") {
        return exists(&target).then(|| string(&target));
    }

    /*
    A Luau file of the same name wins. larvae resolves that spec today, and
    a worm that answered it too would decide the type of a module by which
    of the two ran first.
    */
    for luau in [suffixed(&target, "luau"), target.join("init.luau")] {
        if exists(&luau) {
            return None;
        }
    }

    for luaux in [suffixed(&target, "luaux"), target.join("init.luaux")] {
        if exists(&luaux) {
            return Some(string(&luaux));
        }
    }

    None
}

/// The path a spec names, before the extension search.
///
/// Luau requires by string take three forms, and this reads all three. A
/// spec of any other shape belongs to something else, such as larvae's own
/// `script.Parent` rewriting, and gets no answer here.
fn target(base: &Path, spec: &str) -> Option<PathBuf> {
    if spec.starts_with("./") || spec.starts_with("../") {
        return Some(base.join(spec));
    }

    let rest = spec.strip_prefix('@')?;

    // `@self` is the directory of the requiring file, which is `base`.
    if let Some(rest) = rest.strip_prefix("self/") {
        return Some(base.join(rest));
    }

    let (alias, rest) = match rest.split_once('/') {
        Some((alias, rest)) => (alias, rest),
        None => (rest, ""),
    };

    let (root, path) = alias_path(base, alias)?;

    Some(root.join(path).join(rest))
}

/// The directory an alias of `.luaurc` names, and the `.luaurc` that named it.
///
/// The search walks up from the requiring file, which is what Luau does, so a
/// nested `.luaurc` beats the one at the root of the project.
fn alias_path(base: &Path, alias: &str) -> Option<(PathBuf, String)> {
    for directory in base.ancestors() {
        let text = match std::fs::read_to_string(directory.join(".luaurc")) {
            Ok(text) => text,
            Err(_) => continue,
        };

        // A `.luaurc` that does not parse names no alias, and the spec passes
        // to larvae rather than resolving against a guess.
        let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };

        if let Some(path) = json["aliases"][alias].as_str() {
            return Some((directory.to_path_buf(), path.to_string()));
        }
    }

    None
}

/// The path with an extension put on the end, keeping the name whole.
///
/// `Path::with_extension` replaces what follows the last dot, so it turns
/// `Ui.Card` into `Ui.luaux`. A require spec is a name and not a stem.
fn suffixed(path: &Path, extension: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(".");
    name.push(extension);

    PathBuf::from(name)
}

fn exists(path: &Path) -> bool {
    path.is_file()
}

/// The path as the analyzer holds it: absolute, with `.` and `..` resolved.
fn string(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tree under a fresh directory, so the tests read real files.
    fn tree(name: &str, files: &[&str]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("luaux-worm-requires-{name}"));

        let _ = std::fs::remove_dir_all(&root);

        for file in files {
            let path = root.join(file);
            std::fs::create_dir_all(path.parent().expect("a parent")).expect("a directory");
            std::fs::write(&path, "return nil\n").expect("a file");
        }

        root
    }

    #[test]
    fn a_spec_without_the_extension_finds_the_luaux_file() {
        let root = tree("bare", &["ui/App.luaux", "ui/Card.luaux"]);
        let from = root.join("ui/App.luaux");

        let found = resolve(&from.to_string_lossy(), "./Card").expect("Card.luaux");

        assert!(found.ends_with("Card.luaux"), "{found}");
    }

    #[test]
    fn a_spec_with_the_extension_still_finds_it() {
        let root = tree("spelled", &["ui/App.luaux", "ui/Card.luaux"]);
        let from = root.join("ui/App.luaux");

        assert!(resolve(&from.to_string_lossy(), "./Card.luaux").is_some());
    }

    #[test]
    fn a_directory_answers_with_its_init() {
        let root = tree("init", &["ui/App.luaux", "ui/card/init.luaux"]);
        let from = root.join("ui/App.luaux");

        let found = resolve(&from.to_string_lossy(), "./card").expect("init.luaux");

        assert!(found.ends_with("init.luaux"), "{found}");
    }

    /// The one rule that matters: a Luau module stays larvae's.
    #[test]
    fn a_luau_file_of_the_same_name_passes() {
        let root = tree("luau-wins", &["ui/App.luaux", "ui/Card.luau"]);
        let from = root.join("ui/App.luaux");

        assert_eq!(resolve(&from.to_string_lossy(), "./Card"), None);
        assert_eq!(resolve(&from.to_string_lossy(), "./Card.luau"), None);
    }

    #[test]
    fn a_spec_that_names_nothing_passes() {
        let root = tree("missing", &["ui/App.luaux"]);
        let from = root.join("ui/App.luaux");

        assert_eq!(resolve(&from.to_string_lossy(), "./Nothing"), None);
    }

    #[test]
    fn an_alias_of_luaurc_resolves() {
        let root = tree(
            "alias",
            &["src/client/main.luau", "src/client/ui/Card.luaux"],
        );
        std::fs::write(
            root.join(".luaurc"),
            r#"{ "aliases": { "ui": "src/client/ui" } }"#,
        )
        .expect("a .luaurc");

        let from = root.join("src/client/main.luau");
        let found = resolve(&from.to_string_lossy(), "@ui/Card").expect("Card.luaux");

        assert!(found.ends_with("Card.luaux"), "{found}");
    }

    #[test]
    fn self_names_the_directory_of_the_file() {
        let root = tree("self", &["ui/App.luaux", "ui/Card.luaux"]);
        let from = root.join("ui/App.luaux");

        assert!(resolve(&from.to_string_lossy(), "@self/Card").is_some());
    }

    #[test]
    fn a_require_that_is_not_a_string_spec_passes() {
        let root = tree("other", &["ui/App.luaux", "ui/Card.luaux"]);
        let from = root.join("ui/App.luaux");

        assert_eq!(resolve(&from.to_string_lossy(), "Card"), None);
        assert_eq!(resolve(&from.to_string_lossy(), "script.Parent.Card"), None);
    }

    #[test]
    fn a_name_with_a_dot_in_it_keeps_the_dot() {
        let root = tree("dotted", &["ui/App.luaux", "ui/Ui.Card.luaux"]);
        let from = root.join("ui/App.luaux");

        let found = resolve(&from.to_string_lossy(), "./Ui.Card").expect("Ui.Card.luaux");

        assert!(found.ends_with("Ui.Card.luaux"), "{found}");
    }
}
