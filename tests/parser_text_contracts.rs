use archlens::parser::{count_non_empty_lines, unquote};

#[test]
fn source_size_ignores_whitespace_only_lines() {
    assert_eq!(
        count_non_empty_lines("\r\n  \t\r\nfirst()\r\n \r\nsecond()\r\n"),
        2
    );
    assert_eq!(count_non_empty_lines(" \n\t"), 0);
}

#[test]
fn unquote_removes_only_matching_delimiters() {
    for literal in [r#" "путь/入口" "#, " 'путь/入口' ", " `путь/入口` "] {
        assert_eq!(unquote(literal), "путь/入口");
    }
    for text in [r#""module'"#, "'module`", r#"`module""#, r#""café"#] {
        assert_eq!(unquote(text), text);
    }
}
