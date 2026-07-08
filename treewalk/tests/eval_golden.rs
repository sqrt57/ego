use std::fs;
use std::path::Path;

use treewalk::bootstrap::bootstrap;
use treewalk::eval::{eval_source_print, EgoSignal};

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

        let result = eval_source_print(&source, &ego_path.to_string_lossy(), &mut interp)
            .unwrap_or_else(|sig| {
                let msg = match sig {
                    EgoSignal::Err(e) => e.to_string(),
                    EgoSignal::Exception(_) => "Exception raised".into(),
                    EgoSignal::NonLocalReturn(_, _) => "Non-local return escaped".into(),
                };
                panic!("eval failed for {ego_path:?}: {msg}");
            });

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

fn eval_err(source: &str) -> String {
    let mut interp = bootstrap().unwrap_or_else(|e| panic!("bootstrap failed: {e}"));
    match eval_source_print(source, "<test>", &mut interp) {
        Ok(v) => panic!("expected an error but got: {v:?}"),
        Err(EgoSignal::Err(e)) => e.message,
        Err(sig) => panic!("expected EgoSignal::Err but got a different signal: {}", match sig {
            EgoSignal::Exception(_) => "Exception",
            EgoSignal::NonLocalReturn(_, _) => "NonLocalReturn",
            EgoSignal::Err(_) => unreachable!(),
        }),
    }
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
