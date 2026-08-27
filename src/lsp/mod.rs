/*!
The editor hooks: what luau-xml/lsp does, expressed through larvae's tiers.

Their server owns `.luaux` files and forwards Luau questions to a stock
luau-lsp beside it. Under larvae the forwarding disappears: larvae-lsp
carries the Luau analyzer, tier 1 hands it this worm's lowering, and this
module answers only the markup questions, the way their `hover.rs` and
`completion.rs` do.

The split down the middle is the cursor. Inside a tag the question is about
markup and this worm answers it alone, so the answer replaces the analyzer's
list rather than joining it: a tag position holds a class or a component and
never a Luau global. Everywhere else the file is Luau, the analyzer owns the
answer, and this module passes it through untouched.
*/

pub mod cursor;
mod requires;

use luaux::roblox;

use crate::scan;
use crate::settings::Settings;

pub use requires::resolve;

/*
The lowering for the analyzer, with its maps.

The compiler preserves lines, so the map pairs each line of the lowering
with the same line of the original. The claims are the markup segments:
inside one, this worm answers hover and completion; outside, the file is
Luau and default answers hold.
*/
/// One span of the source, against the span it became in the lowering
pub type SpanMap = Vec<(u32, u32, u32, u32)>;

/// One span of the source that holds markup and not Luau
pub type Claims = Vec<(u32, u32)>;

pub fn load(source: &str, lowered: &str) -> (SpanMap, Claims) {
    let mut span_map = Vec::new();
    let mut gen_at = 0u32;
    let mut orig_at = 0u32;
    let mut orig_lines = source.split_inclusive('\n');

    for gen_line in lowered.split_inclusive('\n') {
        let orig_line = orig_lines.next().unwrap_or("");
        let gen_end = gen_at + gen_line.len() as u32;
        let orig_end = orig_at + orig_line.len() as u32;

        span_map.push((gen_at, gen_end, orig_at, orig_end));
        gen_at = gen_end;
        orig_at = orig_end;
    }

    /*
    The claims are the markup elements: inside one, this worm answers
    hover and completion; outside, the file is Luau and default answers
    hold. The node walk gives each element's span.
    */
    let mut claims: Vec<(u32, u32)> = Vec::new();

    scan::each_node(source, &mut |node| {
        let span = match node {
            luaux::markup::Node::Element(element) => element.span,

            luaux::markup::Node::Fragment(fragment) => fragment.span,
        };

        claims.push((span.start as u32, span.end as u32));
    });

    claims.sort();
    claims.dedup();

    (span_map, claims)
}

/// What a tag names.
enum Tag {
    /// A Roblox class, under the name the project writes for it
    Class(String),
    /// A name bound in this file, which the element calls
    Component,
    /// Neither, which the compiler will report
    Unknown,
}

/// What the tag written here names, in this project.
fn tag(written: &str, text: &str, settings: &Settings) -> Tag {
    let config = &settings.config;

    // A dotted name is always a component; no Roblox class holds a dot.
    if written.contains('.') {
        return Tag::Component;
    }

    // A project alias wins over the class list, because a rename retires the
    // name it replaced and `<TextLabel>` is then an error. That error is what
    // the `Err` arm is: a name the project renamed away.
    match config.resolve_element(written) {
        Ok(Some(class)) => return Tag::Class(class.to_string()),

        Err(_) => return Tag::Unknown,

        Ok(None) => {}
    }

    if roblox::is_class(written) {
        return Tag::Class(written.to_string());
    }

    if components(text).contains(&written.to_string()) {
        return Tag::Component;
    }

    Tag::Unknown
}

/// The hover their server gives a tag or an attribute.
pub fn hover(context: &serde_json::Value, settings: &Settings) -> Option<serde_json::Value> {
    let text = context["text"].as_str()?;
    let offset = context["offset"].as_u64()? as usize;

    let value = match cursor::spot(text, offset) {
        cursor::Spot::Tag { name, .. } if !name.is_empty() => match tag(&name, text, settings) {
            Tag::Class(class) if class == name => format!(
                "```luaux\n<{name}>\n```\nA creatable Roblox class; the element builds one instance."
            ),

            Tag::Class(class) => format!(
                "```luaux\n<{name}>\n```\n`{class}`, under the name this project writes for it."
            ),

            Tag::Component => {
                format!(
                    "```luaux\n<{name}>\n```\nA component bound in this file; the element calls it."
                )
            }

            Tag::Unknown => format!(
                "```luaux\n<{name}>\n```\nNo class or component of this name is in sight; the compiler will say so."
            ),
        },

        cursor::Spot::Attribute {
            tag: written, name, ..
        } if !name.is_empty() => {
            let Tag::Class(class) = tag(&written, text, settings) else {
                // A component takes whatever props it declares, and this
                // worm is not the half that knows a Luau type.
                return None;
            };

            let canonical = settings.config.resolve_property(&class, &name).ok()?;

            if roblox::has_property(&class, &canonical) {
                format!("```luaux\n{name}\n```\nA property of `{class}`.")
            } else if roblox::is_event(&class, &canonical) {
                format!("```luaux\n{name}\n```\nAn event of `{class}`; the value is the handler.")
            } else {
                format!("```luaux\n{name}\n```\n`{class}` has no property or event of this name.")
            }
        }

        _ => return None,
    };

    Some(serde_json::json!({
        "contents": { "kind": "markdown", "value": value }
    }))
}

/*
Completions, the reason their `completion.rs` exists.

Inside a tag the list is this worm's alone. Typing `<Fra` offers Frame and
every other creatable class beside the components of the file, and typing a
name inside the tag offers the properties and the events of that class. The
analyzer's list is dropped there, because a Luau global is never the answer
to either question, and 478 of them buried the 20 that were.

Everywhere else the base list passes through, so the file is Luau outside its
markup and nothing this worm does narrows it.
*/
pub fn completions(
    context: &serde_json::Value,
    base: serde_json::Value,
    settings: &Settings,
) -> serde_json::Value {
    let Some(text) = context["text"].as_str() else {
        return base;
    };

    let Some(offset) = context["offset"].as_u64().map(|o| o as usize) else {
        return base;
    };

    match cursor::spot(text, offset) {
        cursor::Spot::Tag { prefix, .. } => serde_json::json!(tags(&prefix, text, settings)),

        cursor::Spot::Attribute {
            tag: written,
            prefix,
            taken,
            ..
        } => match tag(&written, text, settings) {
            Tag::Class(class) => {
                serde_json::json!(members(&class, &prefix, &taken, settings))
            }

            // A component's props are a Luau type, and the analyzer holds
            // the types. Nothing here beats what it already said.
            _ => base,
        },

        cursor::Spot::Elsewhere => base,
    }
}

/// The classes and components that a tag name may become.
fn tags(prefix: &str, text: &str, settings: &Settings) -> Vec<serde_json::Value> {
    let config = &settings.config;
    let mut items = Vec::new();

    for component in components(text) {
        if component.starts_with(prefix) {
            items.push(serde_json::json!({
                "label": component,
                "kind": 3,
                "detail": "component in this file",
                "sortText": format!("1{component}"),
            }));
        }
    }

    for class in roblox::creatable_classes() {
        let written = settings.spelling.element(config, class);

        if written.starts_with(prefix) {
            items.push(serde_json::json!({
                "label": written,
                "kind": 7,
                "detail": format!("creatable class ({class})"),
                "sortText": format!("3{written}"),
            }));
        }
    }

    items
}

/// The properties and the events of one class, less the ones already written.
fn members(
    class: &str,
    prefix: &str,
    taken: &[String],
    settings: &Settings,
) -> Vec<serde_json::Value> {
    let mut items = Vec::new();

    for property in roblox::properties(class) {
        offer(
            &mut items,
            settings,
            class,
            property,
            prefix,
            taken,
            Member::Property,
        );
    }

    for event in roblox::events(class) {
        offer(
            &mut items,
            settings,
            class,
            event,
            prefix,
            taken,
            Member::Event,
        );
    }

    items
}

/// The two things an attribute can name.
#[derive(Clone, Copy)]
enum Member {
    Property,
    Event,
}

/// Puts one member on the list, under the name the project writes for it.
///
/// A deprecated spelling stays on the list and is marked, rather than
/// dropped. Roblox keeps `brickColor` beside `BrickColor`, and a file that
/// already uses the old one has to be able to finish the word.
fn offer(
    items: &mut Vec<serde_json::Value>,
    settings: &Settings,
    class: &str,
    canonical: &str,
    prefix: &str,
    taken: &[String],
    member: Member,
) {
    let written = settings
        .spelling
        .property(&settings.config, class, canonical);

    if !written.starts_with(prefix) || taken.iter().any(|name| name == &written) {
        return;
    }

    let (kind, what) = match member {
        Member::Property => (10, "property"),
        Member::Event => (23, "event"),
    };

    let deprecated = roblox::is_deprecated(canonical);
    let rank = match (deprecated, member) {
        (true, _) => '9',
        (false, Member::Property) => '1',
        (false, Member::Event) => '2',
    };

    let mut item = serde_json::json!({
        "label": written,
        "kind": kind,
        "detail": format!("{what} of {class}"),
        "sortText": format!("{rank}{written}"),
    });

    if deprecated {
        item["tags"] = serde_json::json!([1]);
    }

    items.push(item);
}

/*
The components bound in the file, by the convention their server reads:
an uppercase name bound by a local or a function. A textual scan is
enough, because a component used as a tag is in scope by name, and a
completion runs on a keystroke, where parsing the file does not.
*/
fn components(text: &str) -> Vec<String> {
    let mut out = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim_start();

        for lead in ["local function ", "local ", "const ", "function "] {
            if let Some(rest) = trimmed.strip_prefix(lead) {
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();

                if name.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
                    out.push(name);
                }

                break;
            }
        }
    }

    out.sort();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const CARD: &str = "local function Card(props)\n\treturn (\n\t\t<Frame Size={props.Size}>\n\t\t\t<TextLabel Text={props.Title} />\n\t\t</Frame>\n\t)\nend\n\nreturn Card\n";

    /// The context larvae sends, with `|` marking the cursor.
    fn context(marked: &str) -> (serde_json::Value, String) {
        let offset = marked.find('|').expect("a cursor");
        let text = marked.replace('|', "");

        (serde_json::json!({ "text": text, "offset": offset }), text)
    }

    fn settings() -> Settings {
        Settings::default()
    }

    fn labels(items: &serde_json::Value) -> Vec<String> {
        items
            .as_array()
            .expect("a list")
            .iter()
            .map(|item| item["label"].as_str().unwrap_or_default().to_string())
            .collect()
    }

    #[test]
    fn the_line_map_pairs_every_line() {
        let (span_map, _) = load("a\nbb\n", "x\nyy\n");

        assert_eq!(span_map, vec![(0, 2, 0, 2), (2, 5, 2, 5)]);
    }

    #[test]
    fn markup_elements_are_the_claims() {
        let (_, claims) = load(CARD, CARD);

        assert!(!claims.is_empty(), "the markup is claimed");
        assert!(claims.iter().all(|(start, end)| start < end));
    }

    #[test]
    fn a_tag_hover_names_the_class_or_component() {
        let (frame, _) = context(&CARD.replace("<Frame Size", "<Fra|me Size"));
        let class = hover(&frame, &settings()).expect("a hover");

        assert!(
            class["contents"]["value"]
                .as_str()
                .expect("markdown")
                .contains("creatable Roblox class"),
            "{class}"
        );

        let (card, _) = context("local function Card(props)\nend\nlocal x = (<Ca|rd />)\n");
        let component = hover(&card, &settings()).expect("a hover");

        assert!(
            component["contents"]["value"]
                .as_str()
                .expect("markdown")
                .contains("component"),
            "{component}"
        );
    }

    #[test]
    fn an_attribute_hover_names_the_property_and_its_class() {
        let (at, _) = context("local x = (<Frame Bac|kgroundTransparency={0} />)\n");
        let answer = hover(&at, &settings()).expect("a hover");

        let text = answer["contents"]["value"].as_str().expect("markdown");

        assert!(text.contains("property of `Frame`"), "{text}");
    }

    #[test]
    fn an_event_hover_says_it_is_an_event() {
        let (at, _) = context("local x = (<TextButton Activa|ted={f} />)\n");
        let answer = hover(&at, &settings()).expect("a hover");

        let text = answer["contents"]["value"].as_str().expect("markdown");

        assert!(text.contains("event of `TextButton`"), "{text}");
    }

    /*
    The list a tag position gives is this worm's alone. A Luau global is
    never the answer, and the base list is 478 of them.
    */
    #[test]
    fn tag_completions_replace_the_base_list() {
        let (context, _) = context("local x = (<Fra|)\n");
        let base = serde_json::json!([{ "label": "print" }]);

        let items = completions(&context, base, &settings());
        let labels = labels(&items);

        assert!(labels.contains(&String::from("Frame")), "{labels:?}");
        assert!(!labels.contains(&String::from("print")), "{labels:?}");
    }

    #[test]
    fn tag_completions_offer_the_components_of_the_file() {
        let (context, _) = context("local function Card(props)\nend\nlocal x = (<Ca|)\n");

        let labels = labels(&completions(&context, serde_json::json!([]), &settings()));

        assert!(labels.contains(&String::from("Card")), "{labels:?}");
    }

    #[test]
    fn attribute_completions_offer_the_members_of_the_class() {
        let (context, _) = context("local x = (<Frame Backgr|)\n");

        let labels = labels(&completions(&context, serde_json::json!([]), &settings()));

        assert!(
            labels.contains(&String::from("BackgroundColor3")),
            "{labels:?}"
        );
        assert!(
            labels.contains(&String::from("BackgroundTransparency")),
            "{labels:?}"
        );
    }

    #[test]
    fn attribute_completions_offer_events_too() {
        let (context, _) = context("local x = (<TextButton Activ|)\n");

        let labels = labels(&completions(&context, serde_json::json!([]), &settings()));

        assert!(labels.contains(&String::from("Activated")), "{labels:?}");
    }

    /// An attribute already written is not offered a second time.
    #[test]
    fn an_attribute_already_on_the_tag_is_not_offered_again() {
        let (context, _) = context("local x = (<Frame Visible={true} Visi|)\n");

        let labels = labels(&completions(&context, serde_json::json!([]), &settings()));

        assert!(!labels.contains(&String::from("Visible")), "{labels:?}");
    }

    /*
    A component takes a Luau type and this worm does not read types, so the
    analyzer's answer stands rather than being replaced by nothing.
    */
    #[test]
    fn a_components_attributes_stay_with_the_analyzer() {
        let (context, _) = context("local function Card(props)\nend\nlocal x = (<Card Ti|)\n");
        let base = serde_json::json!([{ "label": "Title" }]);

        let items = completions(&context, base.clone(), &settings());

        assert_eq!(items, base);
    }

    #[test]
    fn outside_markup_the_base_response_passes_through() {
        let (context, _) = context("local count = 1\nlocal more = cou|\n");
        let base = serde_json::json!([{ "label": "count" }]);

        assert_eq!(completions(&context, base.clone(), &settings()), base);
        assert!(hover(&context, &settings()).is_none());
    }

    /// A project that renames a class completes the name it renamed it to.
    #[test]
    fn a_rename_completes_under_the_name_the_project_writes() {
        let (settings, _) = crate::settings::read("", "{}", "").expect("settings with no luaux.toml");
        let mut settings = settings;
        settings.config =
            luaux::Config::parse("[elements]\nTextLabel = \"text\"\n").expect("a config");
        settings.spelling = crate::settings::Spelling::read("[elements]\nTextLabel = \"text\"\n");

        let (context, _) = context("local x = (<te|)\n");
        let labels = labels(&completions(&context, serde_json::json!([]), &settings));

        assert!(labels.contains(&String::from("text")), "{labels:?}");
        assert!(!labels.contains(&String::from("TextLabel")), "{labels:?}");
    }
}
