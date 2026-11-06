//! mtld3d's grammar.
//!
//! Inner spaces kept, outer quotes dropped, comments and blanks skipped, a line without `=` marked.

use super::*;

#[test]
fn mtld3d_grammar_keeps_inner_spaces_and_drops_outer_quotes() {
    assert_eq!(
        item(" log.dir = \"a b\" "),
        Some(Item::Pair {
            key: b"log.dir".to_vec(),
            value: b"a b".to_vec()
        })
    );
    assert_eq!(
        item("x = a b"),
        Some(Item::Pair {
            key: b"x".to_vec(),
            value: b"a b".to_vec()
        })
    );
}

#[test]
fn mtld3d_grammar_marks_a_line_without_equals() {
    assert_eq!(item("nonsense"), Some(Item::Malformed));
    assert_eq!(item("# x = 1"), None);
    assert_eq!(item("   "), None);
}
