//! The findings that the luaux compiler gives while it builds the file.

use larvae_worm::native::Finding;
use luaux::compile::{CompileError, Warning, compile_recovering};
use luaux::config::LintLevel;
use luaux::{Config, Vide};

use crate::report;

/// Markup in a child expression that no function encloses. The code builds the
/// child one time, so the condition around it looks live and is not.
const STATIC_CONDITIONAL_CHILD: &str = "static_conditional_child";

/// A warning of the luaux compiler that this worm has no more exact name for.
/// A finding under a general name is better than a finding under a wrong name.
const COMPILE_WARNING: &str = "compile_warning";

/// An element or a property that the luaux resolver does not know.
const UNRESOLVED_NAME: &str = "unresolved_name";

/// A problem that stops the compiler, such as an unclosed element.
const COMPILE_ERROR: &str = "compile_error";

/// Compiles the file, and reports what the compiler says about it.
///
/// The error is a finding as well, and not a message about the whole file. The
/// editor sends a file on every keystroke, and a file that somebody is typing
/// is broken most of the time, so the reader wants the mark under the byte that
/// is wrong.
pub fn findings(src: &str, config: &Config, holes: &[usize]) -> Result<Vec<Finding>, Finding> {
    let mut config = config.clone();

    // The host owns the level of a finding: `off` hides a finding that larvae
    // is able to show, and `error` stops the compile at the first one. So the
    // compiler reports this lint as a warning at all times, and larvae decides
    // what the warning means.
    config.static_conditional_child = LintLevel::Warn;

    // A parse error stops the compile, because a recovery from `<Frame` with no
    // `>` is a guess about what the author means. A resolution error does not:
    // the tree is complete, so the file gives every other finding as well.
    let compiled = compile_recovering(src, &Vide, config).map_err(|error| {
        finding(
            COMPILE_ERROR,
            src,
            holes,
            error.offset,
            error.length.max(1),
            &error.message,
            error.help.as_deref(),
        )
    })?;

    let mut findings = Vec::new();

    for warning in &compiled.warnings {
        findings.push(from_warning(src, holes, warning));
    }

    for error in &compiled.errors {
        findings.push(from_error(src, holes, error));
    }

    Ok(findings)
}

fn from_warning(src: &str, holes: &[usize], warning: &Warning) -> Finding {
    finding(
        name_of(warning),
        src,
        holes,
        warning.offset,
        warning.length,
        &warning.message,
        warning.help.as_deref(),
    )
}

fn from_error(src: &str, holes: &[usize], error: &CompileError) -> Finding {
    finding(
        UNRESOLVED_NAME,
        src,
        holes,
        error.offset,
        error.length,
        &error.message,
        error.help.as_deref(),
    )
}

/// The lint that a warning belongs to.
///
/// luaux has one warning today and gives no name with it, so the message is the
/// only mark. A warning that a later version of luaux adds still reaches the
/// user, under the general name.
fn name_of(warning: &Warning) -> &'static str {
    if warning.message.contains("built once") {
        STATIC_CONDITIONAL_CHILD
    } else {
        COMPILE_WARNING
    }
}

fn finding(
    lint: &str,
    src: &str,
    holes: &[usize],
    offset: usize,
    length: usize,
    message: &str,
    help: Option<&str>,
) -> Finding {
    let (start, end) = span(src, holes, offset, length, message);
    let finding = Finding::new(lint, (start as u32, end as u32), message);

    match help {
        Some(help) => finding.with_help(help),
        None => finding,
    }
}

/// The byte range in the file that a finding is about.
///
/// luaux compiles the expression inside a hole as a source of its own, so a
/// finding from inside a hole counts from the start of that expression, and not
/// from the start of the file. The worm knows where each hole starts, so it
/// tries each hole and keeps the first one where the source agrees with the
/// message.
fn span(src: &str, holes: &[usize], offset: usize, length: usize, message: &str) -> (usize, usize) {
    if fits(src, offset, offset + length, message) {
        return (offset, offset + length);
    }

    for hole in holes {
        let start = hole + offset;

        if fits(src, start, start + length, message) {
            return (start, start + length);
        }
    }

    // No hole agrees. Keep what luaux reports, inside the file, because a
    // finding at the wrong position is still better than no finding at all.
    marked(src, offset, offset + length)
}

/// The range inside the file, and never a range of no bytes.
///
/// An editor marks the bytes of a finding, so a finding of no bytes marks
/// nothing. A message at the end of the file, such as the one about an element
/// that no tag closes, points one byte back to the last byte of the file.
fn marked(src: &str, start: usize, end: usize) -> (usize, usize) {
    let start = report::boundary(src, start);
    let end = report::boundary(src, end.max(start + 1));

    if start < end || start == 0 {
        return (start, end);
    }

    (report::boundary(src, start - 1), start)
}

/// Whether the source at this range agrees with the message.
///
/// Each message of luaux holds the text that it is about, and a finding about a
/// child expression covers the hole from the first brace to the last.
fn fits(src: &str, start: usize, end: usize, message: &str) -> bool {
    if start > end || end > src.len() || !src.is_char_boundary(start) || !src.is_char_boundary(end)
    {
        return false;
    }

    let text = &src[start..end];

    message.contains(text) || (text.starts_with('{') && text.ends_with('}'))
}

#[cfg(test)]
mod tests {
    use super::super::tests::IMPORT;
    use super::*;

    fn findings_of(src: &str) -> Vec<Finding> {
        findings(src, &Config::default(), &crate::scan::marks(src).holes).expect("the compiler")
    }

    #[test]
    fn a_file_that_does_not_compile_gives_a_finding_and_not_a_message() {
        let src = "return <Frame>\n";
        let refusal =
            findings(src, &Config::default(), &[]).expect_err("the compiler stops on this one");

        assert_eq!(refusal.lint, COMPILE_ERROR);
        assert!(refusal.message.contains("unclosed element"), "{refusal:?}");
        // A span of no bytes marks nothing in an editor.
        assert!(refusal.span.1 > refusal.span.0, "{refusal:?}");
    }

    #[test]
    fn reports_markup_that_a_condition_builds_one_time() {
        let src = &format!("{IMPORT}return <Frame>{{show and <TextLabel/> or nil}}</Frame>\n");
        let findings = findings_of(src);

        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].lint, STATIC_CONDITIONAL_CHILD);
        assert!(findings[0].help.is_some());

        let (start, end) = findings[0].span;

        assert_eq!(
            &src[start as usize..end as usize],
            "{show and <TextLabel/> or nil}"
        );
    }

    #[test]
    fn reports_a_name_that_luaux_cannot_resolve() {
        let findings = findings_of(&format!("{IMPORT}return <Frmae/>\n"));

        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].lint, UNRESOLVED_NAME);
        assert!(findings[0].message.contains("Frmae"), "{findings:?}");
    }

    #[test]
    fn puts_a_finding_from_inside_a_hole_at_the_right_place() {
        // luaux counts this one from the start of the expression in the hole,
        // and the worm moves it back into the file.
        let src = &format!(
            "{IMPORT}return <Frame>{{items:map(function() return <Frmae/> end)}}</Frame>\n"
        );
        let findings = findings_of(src);

        assert_eq!(findings.len(), 1, "{findings:?}");

        let (start, end) = findings[0].span;
        assert_eq!(&src[start as usize..end as usize], "<Frmae");
    }

    #[test]
    fn markup_inside_a_function_is_not_a_finding() {
        // Vide runs the function again, which is how a reactive child works.
        let src = format!(
            "{IMPORT}return <Frame>{{items:map(function() return <TextLabel/> end)}}</Frame>\n"
        );

        assert!(findings_of(&src).is_empty());
    }
}
