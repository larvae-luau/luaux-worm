//! Messages that larvae shows to a person.
//!
//! larvae prints the string that an operation returns, and it knows the name of
//! the file only. Thus the worm puts the position in the message itself.

use luaux::CompileError;

/// A message with the position of the byte offset in front of it.
pub fn at(src: &str, message: &str, offset: usize, help: Option<&str>) -> String {
    let head = &src[..boundary(src, offset)];
    let line = head.matches('\n').count() + 1;
    let column = head.rsplit('\n').next().unwrap_or("").chars().count() + 1;

    match help {
        Some(help) => format!("line {line}, column {column}: {message}\nhelp: {help}"),
        None => format!("line {line}, column {column}: {message}"),
    }
}

/// The same, for an error of the luaux compiler.
pub fn compile_error(src: &str, error: &CompileError) -> String {
    at(src, &error.message, error.offset, error.help.as_deref())
}

/// The offset, moved back to the start of the character that holds it.
///
/// A panic stops the worm, and larvae loses the whole run with it. A byte
/// offset that a message carries is not always the start of a character, so
/// each cut of the source goes through here.
pub fn boundary(src: &str, offset: usize) -> usize {
    let mut offset = offset.min(src.len());

    while !src.is_char_boundary(offset) {
        offset -= 1;
    }

    offset
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_the_line_and_the_column() {
        let src = "local a = 1\nlocal b = <Frame\n";

        assert_eq!(
            at(src, "expected `>`", 27, None),
            "line 2, column 16: expected `>`"
        );
    }

    #[test]
    fn adds_the_help_line_when_there_is_one() {
        assert_eq!(
            at("x", "no", 0, Some("do this")),
            "line 1, column 1: no\nhelp: do this"
        );
    }
}
