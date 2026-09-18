//! End-to-end test of `ita definition <path>:<line>:<col>`: real binary,
//! real temp files, no library shortcuts.

use std::process::Command;

fn run(dir: &std::path::Path, arg: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ita"))
        .arg("definition")
        .arg(arg)
        .current_dir(dir)
        .output()
        .expect("run `ita definition`")
}

#[test]
fn resolves_instance_method_call() {
    let dir = std::env::temp_dir().join(format!("itaruby-def-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("greeter.rb");
    // `greet` call is on line 6, column 5 (1-based, points at the `g` in
    // `Greeter.new.greet`).
    std::fs::write(
        &path,
        "class Greeter\n  def greet\n    \"hi\"\n  end\nend\n\nGreeter.new.greet\n",
    )
    .unwrap();

    let out = run(&dir, &format!("{}:7:14", path.display()));
    assert!(out.status.success(), "exit status: {:?}, stderr: {}", out.status, String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8(out.stdout).unwrap();
    let expected_path = std::fs::canonicalize(&path).unwrap();
    assert_eq!(stdout.trim(), format!("{}:2:7", expected_path.display()));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn unknown_for_open_class() {
    let dir = std::env::temp_dir().join(format!("itaruby-def-cli-open-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("widget.rb");
    std::fs::write(
        &path,
        "class Widget\n  def method_missing(name, *args)\n    nil\n  end\nend\n\nWidget.new.nonexistent_method\n",
    )
    .unwrap();

    let out = run(&dir, &format!("{}:7:13", path.display()));
    assert!(out.status.success(), "exit status: {:?}, stderr: {}", out.status, String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8(out.stdout).unwrap().trim(), "unknown");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn invalid_argument_exits_2() {
    let out = Command::new(env!("CARGO_BIN_EXE_ita"))
        .arg("definition")
        .arg("not-a-valid-location")
        .output()
        .expect("run `ita definition`");
    assert_eq!(out.status.code(), Some(2), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert!(!String::from_utf8_lossy(&out.stderr).is_empty(), "expected an error message on stderr");
}
