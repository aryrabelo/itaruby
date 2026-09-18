//! End-to-end tests of `ita hover <path>:<line>:<col>`: real binary, real
//! temp files, no library shortcuts. Mirrors `definition_cli.rs` — the CLI
//! is the cheap, scriptable twin of the LSP hover (same `hover_at` query,
//! same honesty contract: no info prints `unknown`, never a guess).

use std::process::Command;

fn run(dir: &std::path::Path, arg: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ita"))
        .arg("hover")
        .arg(arg)
        .current_dir(dir)
        .output()
        .expect("run `ita hover`")
}

fn tempdir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("itaruby-hover-cli-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn literal_typed_local_and_class_new_local() {
    let dir = tempdir("local");
    let path = dir.join("locals.rb");
    std::fs::write(
        &path,
        "class Box\n  def initialize\n    @inner = \"hi\"\n  end\nend\n\nbox = Box.new\ncount = 42\n",
    )
    .unwrap();

    // `count` (line 8, col 1) is typed by its Integer literal.
    let out = run(&dir, &format!("{}:8:1", path.display()));
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8(out.stdout).unwrap().trim(), "Integer");

    // `box` (line 7, col 1) is typed Instance(Box).
    let out = run(&dir, &format!("{}:7:1", path.display()));
    assert_eq!(String::from_utf8(out.stdout).unwrap().trim(), "Box");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn is_a_narrowed_parameter_hovers_the_narrowed_type() {
    let dir = tempdir("narrow");
    let path = dir.join("narrow.rb");
    std::fs::write(
        &path,
        "class Item\n  def bar\n    1\n  end\nend\n\nclass Wrap\n  def check(x)\n    if x.is_a?(Item)\n      x.bar\n    end\n    nil\n  end\nend\n",
    )
    .unwrap();

    // `x` inside the true branch (line 10, col 7) is narrowed to Item by
    // the `is_a?` — hover must show the narrowed type, not Unknown.
    let out = run(&dir, &format!("{}:10:7", path.display()));
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8(out.stdout).unwrap().trim(), "Item");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn typed_ivar_hovers_its_inferred_type() {
    let dir = tempdir("ivar");
    let path = dir.join("ivar.rb");
    std::fs::write(
        &path,
        "class Label\n  def initialize\n    @name = \"hello\"\n  end\n\n  def describe\n    @name\n  end\nend\n",
    )
    .unwrap();

    // `@name` in `describe` (line 7, col 5): union of every write — String.
    let out = run(&dir, &format!("{}:7:5", path.display()));
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8(out.stdout).unwrap().trim(), "String");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn resolved_method_call_hovers_signature_with_def_site() {
    let dir = tempdir("sig");
    let path = dir.join("sig.rb");
    std::fs::write(
        &path,
        "class Calc\n  def add(a, b)\n    a + b\n  end\nend\n\ncalc = Calc.new\ncalc.add(1, 2)\n",
    )
    .unwrap();

    // Hover on `add` in `calc.add(1, 2)` (line 8, col 6): name, arity, and
    // the def site (line 2 in this file). `add(a, b)` has 2 required
    // params, no optional, no rest — arity "2".
    let out = run(&dir, &format!("{}:8:6", path.display()));
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8(out.stdout).unwrap();
    let expected_path = std::fs::canonicalize(&path).unwrap();
    assert_eq!(
        stdout.trim(),
        format!("add — arity 2 — {}:2", expected_path.display()),
        "hover on a resolved call must show name, arity, def site"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn unknown_receiver_and_untyped_param_stay_honest() {
    let dir = tempdir("unknown");
    let path = dir.join("unknown.rb");
    std::fs::write(&path, "class Opaque\n  def dyn(p)\n    p.thing\n  end\nend\n").unwrap();

    // `p` is an untyped parameter: no type info, so `unknown` — never a
    // guess (invariant #1 extended to hover).
    let out = run(&dir, &format!("{}:3:5", path.display()));
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8(out.stdout).unwrap().trim(), "unknown");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn constant_write_hovers_its_value_type() {
    let dir = tempdir("const");
    let path = dir.join("const.rb");
    std::fs::write(&path, "VERSION = \"1.0\"\n").unwrap();

    // `VERSION`'s write (line 1, col 1) infers its RHS: String. (A value
    // constant READ types as Unknown today — hover reuses inference, it
    // does not extend it — so the write is the honest probe here.)
    let out = run(&dir, &format!("{}:1:1", path.display()));
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8(out.stdout).unwrap().trim(), "String");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn invalid_argument_exits_2() {
    let out = Command::new(env!("CARGO_BIN_EXE_ita"))
        .arg("hover")
        .arg("not-a-valid-location")
        .output()
        .expect("run `ita hover`");
    assert_eq!(out.status.code(), Some(2), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert!(!String::from_utf8_lossy(&out.stderr).is_empty(), "expected an error message on stderr");
}
