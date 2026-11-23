/// The two warnings the parser returns log the text it logged itself.
#[test]
fn the_returned_warnings_log_the_text_the_parser_used_to() {
    use super::{DxsoWarning, warning_text};
    assert_eq!(
        warning_text(DxsoWarning::LinkedInputsOverflow),
        "dxso: PS declares more than 8 extra input semantics → the rest read zero"
    );
    assert_eq!(
        warning_text(DxsoWarning::ColorOutBeyondTargets(5)),
        "dxso: PS writes oC5, beyond the four D3D9 render targets → write ignored"
    );
}
