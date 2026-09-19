//! End-to-end checks that run the built binary directly - for behaviour that
//! lives at the CLI boundary (files left on disk) and has no single function
//! worth unit testing in isolation.

use std::path::PathBuf;
use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_imgdupe"))
}

/// A folder with one file that merely has an image extension - imgdupe fails
/// to decode it and reports zero groups, which is all these tests need: the
/// point is what happens to `--csv` / `--delete-script` once the run ends,
/// not the hashing itself.
fn folder_with_one_unreadable_image(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("imgdupe-force-test-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    std::fs::write(dir.join("not-really-a.jpg"), b"not a real image").expect("write fixture file");
    dir
}

#[test]
fn an_existing_csv_report_is_left_untouched_without_force() {
    let dir = folder_with_one_unreadable_image("csv");
    let csv_path = dir.join("out.csv");
    std::fs::write(&csv_path, "sentinel - do not overwrite me").expect("write sentinel");

    let output = bin().arg(&dir).args(["--csv"]).arg(&csv_path).output().expect("run imgdupe");

    let contents = std::fs::read_to_string(&csv_path).expect("csv file should still exist");
    assert_eq!(contents, "sentinel - do not overwrite me", "an existing --csv file must not be overwritten without --force");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--force"), "refusal should explain how to force the overwrite, got: {stderr}");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn force_overwrites_an_existing_csv_report() {
    let dir = folder_with_one_unreadable_image("csv-yes");
    let csv_path = dir.join("out.csv");
    std::fs::write(&csv_path, "sentinel - do not overwrite me").expect("write sentinel");

    let status = bin().arg(&dir).args(["--csv"]).arg(&csv_path).arg("--force").status().expect("run imgdupe");
    assert!(status.success());

    let contents = std::fs::read_to_string(&csv_path).expect("csv file should still exist");
    assert!(contents.starts_with("group,role,hash"), "the real report should have been written over the sentinel: {contents:?}");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn an_existing_delete_script_is_left_untouched_without_force() {
    let dir = folder_with_one_unreadable_image("script");
    let script_path = dir.join("cleanup.sh");
    std::fs::write(&script_path, "sentinel - do not overwrite me").expect("write sentinel");

    let output = bin().arg(&dir).args(["--delete-script"]).arg(&script_path).output().expect("run imgdupe");

    let contents = std::fs::read_to_string(&script_path).expect("script file should still exist");
    assert_eq!(contents, "sentinel - do not overwrite me", "an existing --delete-script file must not be overwritten without --force");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--force"), "refusal should explain how to force the overwrite, got: {stderr}");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn force_overwrites_an_existing_delete_script() {
    let dir = folder_with_one_unreadable_image("script-yes");
    let script_path = dir.join("cleanup.sh");
    std::fs::write(&script_path, "sentinel - do not overwrite me").expect("write sentinel");

    let status = bin().arg(&dir).args(["--delete-script"]).arg(&script_path).arg("--force").status().expect("run imgdupe");
    assert!(status.success());

    let contents = std::fs::read_to_string(&script_path).expect("script file should still exist");
    assert!(contents.starts_with("#!/bin/sh"), "the real script should have been written over the sentinel: {contents:?}");

    std::fs::remove_dir_all(&dir).ok();
}
