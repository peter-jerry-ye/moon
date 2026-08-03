use super::*;

#[test]
fn clean_check_output_contract() {
    let dir = TestDir::new("moon_new/plain");
    moon_cmd(&dir)
        .args(["check", "--sort-input", "-j1"])
        .assert()
        .success()
        .stdout_eq(snapbox::str![[r#"
Finished. moon: ran 4 tasks, now up to date

"#]])
        .stderr_eq(snapbox::str![""]);

    let quiet_dir = TestDir::new("moon_new/plain");
    moon_cmd(&quiet_dir)
        .args(["check", "--sort-input", "-j1", "--quiet"])
        .assert()
        .success()
        .stdout_eq("")
        .stderr_eq("");

    let verbose_dir = TestDir::new("supported_targets_test_target_mismatch.in");
    moon_cmd(&verbose_dir)
        .args([
            "check",
            "--target",
            "js",
            "--sort-input",
            "-j1",
            "--verbose",
        ])
        .assert()
        .success()
        .stdout_eq(snapbox::str![[r#"
Finished. moon: ran 1 task, now up to date

"#]])
        .stderr_eq(snapbox::str![[r#"
Warning: Package `supported/test-target-mismatch/lib` uses legacy array syntax for `supported_targets`; use expression syntax like `<backend>` instead
Warning: Package `supported/test-target-mismatch/nativedep` uses legacy array syntax for `supported_targets`; use expression syntax like `<backend>` instead
Skipping whitebox tests for package `supported/test-target-mismatch/lib` on backend `js`: target is not realizable for this backend. Realizable backends: [native]
Skipping blackbox tests for package `supported/test-target-mismatch/lib` on backend `js`: target is not realizable for this backend. Realizable backends: [native]

"#]]);
}

#[test]
fn failed_check_output_contract() {
    let dir = TestDir::new("dedup_diag_error_limit.in");
    moon_cmd(&dir)
        .args(["check", "--diagnostic-limit", "1", "--sort-input", "-j1"])
        .assert()
        .failure()
        .stdout_eq(snapbox::str![[r#"
Failed with 3 warnings, 1 errors.

"#]])
        .stderr_eq(snapbox::str![[r#"
Error: [4021]
   ╭─[ [..]/z_error.mbt:3:3 ]
   │
 3 │   missing_identifier
   │   ─────────┬────────  
   │            ╰────────── The value identifier missing_identifier is unbound.
───╯
Warning: diagnostic output limited by --diagnostic-limit: 0 errors and 3 warnings were not displayed.
Error: failed when checking project

"#]]);
}

#[test]
fn legacy_json_diagnostic_output_contract() {
    let dir = TestDir::new("dedup_diag_error_limit.in");
    moon_cmd(&dir)
        .args([
            "check",
            "--output-json",
            "--diagnostic-limit",
            "1",
            "--sort-input",
            "-j1",
        ])
        .assert()
        .failure()
        .stdout_eq(snapbox::str![[r#"
{"$message_type":"diagnostic","level":"error","error_code":4021,"path":"[..]/z_error.mbt","loc":"3:3-3:21","message":"The value identifier missing_identifier is unbound.","context":"2 |fn error_source() -> Unit {/n3 |  missing_identifier/n4 |}/n"}

"#]])
        .stderr_eq(snapbox::str![[r#"
Warning: diagnostic output limited by --diagnostic-limit: 0 errors and 3 warnings were not displayed.
Error: failed when checking project

"#]]);
}

#[test]
fn multi_backend_check_output_contract() {
    let dir = TestDir::new("workspace_conflicting_preferred_targets.in");
    moon_cmd(&dir)
        .args(["check", "--sort-input", "-j1"])
        .assert()
        .success()
        .stdout_eq(snapbox::str![[r#"
Finished. moon: ran 2 tasks, now up to date
Finished. moon: ran 2 tasks, now up to date

"#]])
        .stderr_eq(snapbox::str![""]);
}
