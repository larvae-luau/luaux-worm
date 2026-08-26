/*!
The editor hooks: what luau-xml/lsp does, expressed through larvae's tiers.

Their server owns `.luaux` files and forwards Luau questions to a stock
luau-lsp beside it. Under larvae the forwarding disappears: larvae-lsp
carries the Luau analyzer, tier 1 hands it this worm's lowering, and this
module answers only the markup questions, the way their `hover.rs` and
`completion.rs` do.

The require rule the owner set: a require of a Luau file stays larvae's,
untouched. A require of a `.luaux` file resolves here, from Luau and from
luaux files alike, and the analyzer reads the lowering, so hover and
completion carry the component's real type across the boundary.
*/

use crate::creatable;
use crate::scan;

/// A require spec of a `.luaux` file, resolved against the requiring file
pub fn resolve(from: &str, spec: &str) -> Option<String> {
    if !spec.ends_with(".luaux") {
        return None;
    }

    let base = std::path::Path::new(from).parent()?;

    base.join(spec)
        .canonicalize()
        .ok()
        .map(|p| p.to_string_lossy().into_owned())
}

/*
The lowering for the analyzer, with its maps.

The compiler preserves lines, so the map pairs each line of the lowering
with the same line of the original. The claims are the markup segments:
inside one, this worm answers hover and completion; outside, the file is
Luau and default answers hold.
*/
pub fn load(source: &str, lowered: &str) -> (Vec<(u32, u32, u32, u32)>, Vec<(u32, u32)>) {
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

/// The element whose name the offset touches: the tag name and where it opens
fn tag_at(source: &str, offset: usize) -> Option<(String, usize)> {
    let mut hit = None;

    scan::each_node(source, &mut |node| {
        let (name, span) = match node {
            luaux::markup::Node::Element(element) => {
                (element.name.as_written().to_string(), element.span)
            }

            luaux::markup::Node::Fragment(fragment) => (String::new(), fragment.span),
        };

        if name.is_empty() {
            return;
        }

        /*
        The name sits right past the `<` of the element. A cursor on the
        name, `<` included, is a hover of the tag; a cursor deeper inside
        belongs to an inner node, which visits later and wins.
        */
        let name_start = span.start + 1;
        let name_end = name_start + name.len();

        if offset >= span.start && offset <= name_end {
            hit = Some((name, name_start));
        }

        let _ = name_end;
    });

    hit
}

/// The hover their server gives a tag: class or component, and which
pub fn hover(context: &serde_json::Value) -> Option<serde_json::Value> {
    let text = context["text"].as_str()?;
    let offset = context["offset"].as_u64()? as usize;

    let (name, _) = tag_at(text, offset)?;

    let value = if creatable::CLASSES.binary_search(&name.as_str()).is_ok() {
        format!(
            "```luaux\n<{name}>\n```\nA creatable Roblox class; the element builds one instance."
        )
    } else if components(text).contains(&name) {
        format!("```luaux\n<{name}>\n```\nA component bound in this file; the element calls it.")
    } else {
        format!(
            "```luaux\n<{name}>\n```\nNo class or component of this name is in sight; the compiler will say so."
        )
    };

    Some(serde_json::json!({
        "contents": { "kind": "markdown", "value": value }
    }))
}

/*
Tag completions, the reason their `completion.rs` exists: typing `<Fra`
offers Frame and every other creatable class, alongside the components
bound in the file. The base response stays in the list, so a worm earlier
in the pool loses nothing.
*/
pub fn completions(context: &serde_json::Value, base: serde_json::Value) -> serde_json::Value {
    let Some(text) = context["text"].as_str() else {
        return base;
    };

    let Some(offset) = context["offset"].as_u64().map(|o| o as usize) else {
        return base;
    };

    // Only inside an opening tag: the letters after a `<`.
    let head = &text[..offset.min(text.len())];
    let open = match head.rfind('<') {
        Some(at)
            if head[at + 1..]
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_') =>
        {
            at
        }

        _ => return base,
    };

    let prefix = &head[open + 1..];
    let mut items: Vec<serde_json::Value> = base.as_array().cloned().unwrap_or_default();

    for class in creatable::CLASSES {
        if class.starts_with(prefix) {
            items.push(serde_json::json!({
                "label": class,
                "kind": 7,
                "detail": "creatable class",
                "sortText": format!("3{class}"),
            }));
        }
    }

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

    serde_json::json!(items)
}

/*
The components bound in the file, by the convention their server reads:
an uppercase name bound by a local or a function. A textual scan is
enough, because a component used as a tag is in scope by name.
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

    #[test]
    fn only_luaux_specs_resolve() {
        assert!(resolve("/p/src/main.luau", "./other.luau").is_none());
        assert!(resolve("/p/src/main.luau", "./missing.luaux").is_none());
    }

    #[test]
    fn the_line_map_pairs_every_line() {
        let lowered = "line one longer\nline two\n";
        let (map, _) = load("a\nb\n", lowered);

        assert_eq!(map.len(), 2);
        assert_eq!(map[0], (0, 16, 0, 2));
    }

    #[test]
    fn markup_elements_are_the_claims() {
        let (_, claims) = load(CARD, CARD);

        assert!(!claims.is_empty(), "the elements claim their spans");
    }

    #[test]
    fn a_tag_hover_names_the_class_or_component() {
        let at = CARD.find("<Frame").unwrap() + 2;
        let ctx = serde_json::json!({ "text": CARD, "offset": at });
        let hover = hover(&ctx).expect("a tag hovers");

        assert!(
            hover["contents"]["value"]
                .as_str()
                .unwrap()
                .contains("creatable Roblox class"),
            "{hover}"
        );
    }

    #[test]
    fn tag_completions_offer_classes_and_components() {
        let src = "local function Card(props)\nend\nreturn (\n\t<Fra\n)\n";
        let at = src.find("<Fra").unwrap() + 4;
        let ctx = serde_json::json!({ "text": src, "offset": at });
        let items = completions(&ctx, serde_json::json!([]));
        let labels: Vec<&str> = items
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|i| i["label"].as_str())
            .collect();

        assert!(labels.contains(&"Frame"), "{labels:?}");

        // A component matches its own prefix; classes and components rank
        // by their sortText tiers.
        let ctx = serde_json::json!({ "text": src, "offset": src.find("<Fra").unwrap() + 1 });
        let all = completions(&ctx, serde_json::json!([]));
        let has_card = all.as_array().unwrap().iter().any(|i| i["label"] == "Card");

        assert!(has_card, "components offer");
    }

    #[test]
    fn outside_markup_the_base_response_passes_through() {
        let src = "local x = 1\n";
        let ctx = serde_json::json!({ "text": src, "offset": 5 });
        let base = serde_json::json!([{ "label": "keep" }]);

        assert_eq!(completions(&ctx, base.clone()), base);
        assert!(hover(&ctx).is_none());
    }
}
