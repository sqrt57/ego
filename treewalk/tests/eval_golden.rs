use std::fs;
use std::path::Path;
use std::rc::Rc;

use treewalk::bootstrap::{bootstrap, Interpreter};
use treewalk::error::SourceSpan;
use treewalk::eval::{eval_send, eval_source_print, EgoSignal};
use treewalk::object::ObjectKind;

/// Turns any `EgoSignal` into an `EgoError`-shaped (span, message) pair,
/// reading an uncaught exception's `messageText` (lang-spec.md §10) the same
/// way `signal:` set it. Keeps fatal-error assertions working unchanged now
/// that built-in faults route through the exception mechanism instead of a
/// plain `EgoSignal::Err`.
fn signal_to_error(sig: EgoSignal, interp: &mut Interpreter) -> treewalk::error::EgoError {
    match sig {
        EgoSignal::Err(e) => e,
        EgoSignal::Exception(exc_obj, span) => {
            let lookup_span = SourceSpan::new(Rc::new("<test>".to_string()), 0, 0);
            let text = match eval_send(exc_obj, "messageText", &[], &lookup_span, interp) {
                Ok(id) => match &interp.arena.get(id).kind {
                    ObjectKind::StringVal(s) => s.to_string(),
                    _ => "an exception".to_string(),
                },
                Err(_) => "an exception".to_string(),
            };
            treewalk::error::EgoError::new(span, text)
        }
        EgoSignal::NonLocalReturn(_, _) => panic!("expected an error but got a non-local return escape"),
        EgoSignal::HandlerUnwind(_, _) => panic!("expected an error but got a handler-unwind escape"),
        EgoSignal::Resume(_) => panic!("expected an error but got a resume escape"),
    }
}

fn run_golden_dir(dir: &str) {
    let test_dir = Path::new(dir);
    let mut entries: Vec<_> = fs::read_dir(test_dir)
        .unwrap_or_else(|e| panic!("cannot open golden dir {dir}: {e}"))
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "ego").unwrap_or(false))
        .collect();
    entries.sort_by_key(|e| e.path());

    for entry in entries {
        let ego_path = entry.path();
        let expected_path = ego_path.with_extension("expected");

        let source = fs::read_to_string(&ego_path)
            .unwrap_or_else(|e| panic!("cannot read {ego_path:?}: {e}"));
        let expected = fs::read_to_string(&expected_path)
            .unwrap_or_else(|e| panic!("missing expected file for {ego_path:?}: {e}"));

        let mut interp = bootstrap()
            .unwrap_or_else(|e| panic!("bootstrap failed for {ego_path:?}: {e}"));

        let result = match eval_source_print(&source, &ego_path.to_string_lossy(), &mut interp) {
            Ok(v) => v,
            Err(sig) => {
                let e = signal_to_error(sig, &mut interp);
                panic!("eval failed for {ego_path:?}: {}", e.message);
            }
        };

        let actual = result.unwrap_or_default();
        assert_eq!(
            actual.trim_end(),
            expected.trim_end(),
            "mismatch for {ego_path:?}"
        );
    }
}

#[test]
fn golden_1_5_literals() {
    run_golden_dir("tests/eval_golden/1.5-literals");
}

#[test]
fn golden_1_6_objects() {
    run_golden_dir("tests/eval_golden/1.6-objects");
}

#[test]
fn golden_1_7_var_slots() {
    run_golden_dir("tests/eval_golden/1.7-var-slots");
}

#[test]
fn golden_1_8_arithmetic() {
    run_golden_dir("tests/eval_golden/1.8-arithmetic");
}

#[test]
fn golden_1_9_parent_resend() {
    run_golden_dir("tests/eval_golden/1.9-parent-resend");
}

#[test]
fn golden_1_10_blocks() {
    run_golden_dir("tests/eval_golden/1.10-blocks");
}

#[test]
fn golden_1_11_control_flow() {
    run_golden_dir("tests/eval_golden/1.11-control-flow");
}

#[test]
fn golden_1_12_strings() {
    run_golden_dir("tests/eval_golden/1.12-strings");
}

#[test]
fn golden_1_14_cascades() {
    run_golden_dir("tests/eval_golden/1.14-cascades");
}

#[test]
fn golden_1_15_exceptions() {
    run_golden_dir("tests/eval_golden/1.15-exceptions");
}

fn eval_err_full(source: &str, filename: &str) -> treewalk::error::EgoError {
    let mut interp = bootstrap().unwrap_or_else(|e| panic!("bootstrap failed: {e}"));
    match eval_source_print(source, filename, &mut interp) {
        Ok(v) => panic!("expected an error but got: {v:?}"),
        Err(sig) => signal_to_error(sig, &mut interp),
    }
}

fn eval_err(source: &str) -> String {
    eval_err_full(source, "<test>").message
}

#[test]
fn int_add_overflow_is_fatal() {
    let msg = eval_err("9223372036854775807 + 1");
    assert!(msg.contains("overflow"), "got: {msg}");
}

#[test]
fn int_div_by_zero_is_fatal() {
    let msg = eval_err("1 / 0");
    assert!(msg.contains("division by zero"), "got: {msg}");
}

#[test]
fn float_div_by_zero_is_fatal() {
    let msg = eval_err("1.0 / 0.0");
    assert!(msg.contains("division by zero"), "got: {msg}");
}

#[test]
fn mixed_binary_operators_without_parens_is_a_parse_error() {
    let msg = eval_err("3 + 4 * 2");
    assert!(msg.contains('+') && msg.contains('*'), "got: {msg}");
}

#[test]
fn ambiguous_parent_lookup_is_fatal() {
    let msg = eval_err(
        "(| a* = (| greet = ( 1 ) |). b* = (| greet = ( 2 ) |) |) greet",
    );
    assert!(msg.contains("ambiguous"), "got: {msg}");
}

#[test]
fn directed_resend_to_unknown_parent_name_is_fatal() {
    let msg = eval_err(
        "(| parent* = (| greet = ( 1 ) |). \
            greet = ( notAParent.greet ) |) greet",
    );
    assert!(msg.contains("notAParent"), "got: {msg}");
}

#[test]
fn resend_outside_a_method_is_fatal() {
    let msg = eval_err("resend.printString");
    assert!(msg.contains("resend"), "got: {msg}");
}

#[test]
fn dead_block_non_local_return_is_fatal() {
    let msg = eval_err(
        "(| stash <- 0. \
            makeBlock = ( stash: [^ 1]. 2 ). \
            run = ( makeBlock. stash value ) \
         |) run",
    );
    assert!(msg.contains("dead activation"), "got: {msg}");
}

#[test]
fn wrong_arg_count_to_block_is_fatal() {
    let msg = eval_err("[| :x | x] value");
    assert!(msg.contains("expected 1, got 0"), "got: {msg}");
}

#[test]
fn while_true_condition_must_be_boolean_is_fatal() {
    let msg = eval_err("[1] whileTrue: [1]");
    assert!(msg.contains("true or false"), "got: {msg}");
}

#[test]
fn string_concat_with_non_string_argument_is_fatal() {
    let msg = eval_err("'foo' , 3");
    assert!(msg.contains("requires string"), "got: {msg}");
}

// ── Exception handling (substage 1.15) ──────────────────────────────────────

#[test]
fn signal_with_no_handler_is_fatal() {
    let msg = eval_err("zeroDivide signal: 'boom'");
    assert!(msg.contains("boom"), "got: {msg}");
}

#[test]
fn non_matching_on_do_type_still_lets_exception_escape_fatal() {
    let msg = eval_err("[1 / 0] on: messageNotUnderstood Do: [| :e | 'wrong']");
    assert!(msg.contains("division by zero"), "got: {msg}");
}

#[test]
fn resume_outside_a_handler_is_fatal() {
    let msg = eval_err("zeroDivide resume: 1");
    assert!(msg.contains("resume"), "got: {msg}");
}

#[test]
fn return_outside_a_handler_is_fatal() {
    let msg = eval_err("zeroDivide return: 1");
    assert!(msg.contains("return"), "got: {msg}");
}

// ── Error location (substage 1.13) ──────────────────────────────────────────

#[test]
fn error_display_matches_file_line_column_format() {
    // BinarySend's span is the operator's position: "1 / 0" -> `/` at column 3.
    let e = eval_err_full("1 / 0", "path/to/file.ego");
    assert_eq!(e.to_string(), "path/to/file.ego:1:3: error: division by zero");
}

#[test]
fn error_span_points_to_the_line_and_column_of_the_failing_send() {
    // The error must point at `foo` (the failing send) on line 3, not line 1.
    let e = eval_err_full("1.\n2.\nfoo", "<test>");
    assert_eq!((e.span.line, e.span.column), (3, 1));
}

#[test]
fn error_span_column_points_past_leading_whitespace() {
    // "   1 / 0" -> `/` at column 6 (3 leading spaces + "1 " before it).
    let e = eval_err_full("1.\n   1 / 0", "<test>");
    assert_eq!((e.span.line, e.span.column), (2, 6));
}
