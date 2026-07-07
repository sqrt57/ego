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
