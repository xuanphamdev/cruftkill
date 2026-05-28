//! Regression tests for metadata fields in scriptable NDJSON output.

use std::fs;
use std::process::Command;

#[test]
fn no_tui_json_contains_metadata_fields() {
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir_all(tmp.path().join("app/.venv")).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cft"))
        .arg("--no-tui")
        .arg("-p")
        .arg("python")
        .arg(tmp.path())
        .output()
        .expect("run cft");

    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let stdout = String::from_utf8(output.stdout).unwrap();
    let line = stdout.lines().find(|line| !line.trim().is_empty()).expect("one JSON line");
    let value: serde_json::Value = serde_json::from_str(line).expect("valid JSON");

    assert_eq!(value["target_name"], ".venv");
    assert_eq!(value["category"], "virtual-environment");
    assert_eq!(value["delete_risk"], "low");
    assert_eq!(
        value["delete_risk_reason"],
        "Regenerable cache or build output outside sensitive paths"
    );
    assert_eq!(value["is_sensitive"], false);
    assert!(value["rebuild_hint"].as_str().unwrap().contains("Recreate"));
    let ecosystems = value["ecosystems"].as_array().unwrap();
    assert!(ecosystems.iter().any(|name| name == "python"));
    assert!(ecosystems.iter().any(|name| name == "data-science"));
}

#[test]
fn no_tui_json_marks_custom_targets_medium_risk() {
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir_all(tmp.path().join("app/my-cache")).unwrap();

    let value = first_json_line(
        Command::new(env!("CARGO_BIN_EXE_cft"))
            .arg("--no-tui")
            .arg("-t")
            .arg("my-cache")
            .arg(tmp.path()),
    );

    assert_eq!(value["target_name"], "my-cache");
    assert_eq!(value["category"], "unknown");
    assert_eq!(value["delete_risk"], "medium");
    assert_eq!(value["ecosystems"].as_array().unwrap().len(), 0);
    assert!(value["delete_risk_reason"].as_str().unwrap().contains("Custom target"));
}

#[test]
fn no_tui_json_marks_sensitive_paths_high_risk() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    fs::create_dir_all(home.join(".config/tool/node_modules")).unwrap();

    let mut command = Command::new(env!("CARGO_BIN_EXE_cft"));
    command.env("HOME", &home).arg("--no-tui").arg("-p").arg("node").arg(&home);

    let value = first_json_line(&mut command);

    assert_eq!(value["target_name"], "node_modules");
    assert_eq!(value["delete_risk"], "high");
    assert_eq!(value["is_sensitive"], true);
    assert!(value["risk_reason"].as_str().unwrap().contains(".config"));
    assert!(value["delete_risk_reason"].as_str().unwrap().contains(".config"));
}

#[test]
fn no_tui_json_marks_disabled_risk_analysis_as_medium_risk() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    fs::create_dir_all(home.join(".config/tool/node_modules")).unwrap();

    let mut command = Command::new(env!("CARGO_BIN_EXE_cft"));
    command
        .env("HOME", &home)
        .arg("--no-tui")
        .arg("--no-risk-analysis")
        .arg("-p")
        .arg("node")
        .arg(&home);

    let value = first_json_line(&mut command);

    assert_eq!(value["target_name"], "node_modules");
    assert_eq!(value["is_sensitive"], false);
    assert!(value["risk_reason"].is_null());
    assert_eq!(value["delete_risk"], "medium");
    assert!(value["delete_risk_reason"].as_str().unwrap().contains("Risk analysis disabled"));
}

fn first_json_line(command: &mut Command) -> serde_json::Value {
    let output = command.output().expect("run cft");
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let stdout = String::from_utf8(output.stdout).unwrap();
    let line = stdout.lines().find(|line| !line.trim().is_empty()).expect("one JSON line");
    serde_json::from_str(line).expect("valid JSON")
}
