//! bead ita-uo4: `ita check` over many files must emit diagnostics in
//! path order, not in `read_dir` order.
//!
//! The mutation side is real, not hypothetical: the w12-closure blind
//! verdict observed `p8, p2, p5, p1, p3` out of a directory of `pN.rb`
//! probes, which is exactly the shape this fixture plants. Drop the
//! `sort_by` in `run_check` and `many_files_emit_in_path_order` fails on
//! this filesystem; the silence side is `single_file_needs_no_ordering`,
//! which must keep passing either way.

use std::path::PathBuf;
use std::process::Command;

struct TempDirGuard(PathBuf);

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn tmpdir(tag: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("itaruby-order-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn check(dir: &PathBuf) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_ita"))
        .arg("check")
        .arg(dir)
        .output()
        .expect("run `ita check`");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Only the `path:line:col:` header lines, in the order printed.
fn diagnostic_paths(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .filter(|l| l.contains(": warning[") || l.contains(": error["))
        .map(|l| l.split(':').next().unwrap().to_string())
        .collect()
}

#[test]
fn many_files_emit_in_path_order() {
    let dir = tmpdir("many");
    let _guard = TempDirGuard(dir.clone());

    // Written in a scrambled order so creation order can never be
    // mistaken for the sort doing its job. Each file yields exactly one
    // E0104 (unresolved constant), so the header count is the file count.
    for n in [8, 2, 5, 1, 3, 7, 4, 6] {
        std::fs::write(dir.join(format!("p{n}.rb")), format!("Missing{n}::Thing\n"))
            .unwrap();
    }

    let paths = diagnostic_paths(&check(&dir));
    assert_eq!(paths.len(), 8, "one diagnostic per file, got: {paths:?}");

    let mut sorted = paths.clone();
    sorted.sort();
    assert_eq!(paths, sorted, "diagnostics must come out in path order");
}

#[test]
fn single_file_needs_no_ordering() {
    let dir = tmpdir("single");
    let _guard = TempDirGuard(dir.clone());
    std::fs::write(dir.join("only.rb"), "Missing::Thing\n").unwrap();

    let paths = diagnostic_paths(&check(&dir));
    assert_eq!(paths.len(), 1, "got: {paths:?}");
    assert!(paths[0].ends_with("only.rb"), "got: {paths:?}");
}
