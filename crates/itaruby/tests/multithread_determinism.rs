//! bead ita-6dh: `ita check` computes per-file diagnostics on a
//! worker-thread pool by default (`check_worker_count` in `main.rs`),
//! using salsa's own `db.clone()`-per-thread idiom (every clone shares the
//! same underlying storage — see salsa's own `tests/parallel/*.rs`). The
//! hard constraint: parallelizing the COMPUTATION must never perturb the
//! PRINTED order (bead ita-uo4 already sorts `to_check` by path before
//! anything prints) or produce a single differing byte, under any of the
//! three output channels (`normal`, `--format=json`, `--format=agent`) or
//! `--stats`.
//!
//! `ITARUBY_JOBS` is a debug/test-only knob (never a documented `ita
//! check` flag — `parse_check_flags`'s usage string is untouched) that
//! lets this test force the sequential (`=1`) and parallel (`=8`, more
//! workers than any one chunk could hide a race behind) code paths from
//! the SAME binary and diff them directly, instead of comparing against a
//! separately built pre-change binary.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn run_check(dir: &Path, jobs: &str, extra_args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ita"))
        .env("ITARUBY_JOBS", jobs)
        .arg("check")
        .args(extra_args)
        .arg(dir)
        .output()
        .expect("run `ita check`")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn tmpdir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// 60 files across 3 shapes, scrambled numbering so path order can never
/// be mistaken for creation/chunking order doing the work: 20 E0107
/// contradictions (exercises `render_contradiction_block` +
/// `--format=agent`'s constraint channel), 20 E0101 ivar typos (a real
/// `Error`, plus `render_excerpt`'s did-you-mean/defined-at payload path),
/// 20 plain E0104 unresolved constants. Reopening the same class name
/// shape per-`n` keeps every file's own diagnostic self-contained.
fn build_fixture(dir: &Path) {
    for n in 0..20 {
        std::fs::write(
            dir.join(format!("contradiction_{n:02}.rb")),
            format!(
                "class ConstrContradiction{n}\n  def process(x)\n    x.upcase\n    x.push(1)\n  end\nend\n"
            ),
        )
        .unwrap();
        std::fs::write(
            dir.join(format!("typo_{n:02}.rb")),
            format!(
                "class ConstrTypo{n}\n  def initialize\n    @tag = \"hello\"\n  end\n\n  def mutate\n    @tag.push(1)\n  end\nend\n"
            ),
        )
        .unwrap();
        std::fs::write(
            dir.join(format!("missing_{n:02}.rb")),
            format!("Missing{n}::Thing\n"),
        )
        .unwrap();
    }
}

/// Runs `jobs`/`extra_args` twice and asserts the two runs agree with
/// each other (determinism at a fixed worker count) before returning the
/// stdout + exit code for cross-worker-count comparison.
fn stable_run(dir: &Path, jobs: &str, extra_args: &[&str]) -> (String, Option<i32>) {
    let a = run_check(dir, jobs, extra_args);
    let b = run_check(dir, jobs, extra_args);
    let (sa, sb) = (stdout(&a), stdout(&b));
    assert_eq!(
        sa, sb,
        "jobs={jobs} args={extra_args:?}: two runs at the same worker count disagree"
    );
    assert_eq!(
        a.status.code(),
        b.status.code(),
        "jobs={jobs} args={extra_args:?}: exit code differs between two runs at the same worker count"
    );
    (sa, a.status.code())
}

/// B.1: for every output channel, 1 worker vs 8 workers over the same 60
/// files is byte-identical stdout and an identical exit code.
#[test]
fn parallel_output_is_byte_identical_to_sequential() {
    let channels: [(&str, &[&str]); 4] = [
        ("normal", &[]),
        ("json", &["--format=json"]),
        ("agent", &["--format=agent"]),
        ("stats", &["--stats"]),
    ];
    for (label, extra_args) in channels {
        let dir = tmpdir(&format!("mt_determinism_{label}"));
        build_fixture(&dir);

        let (single, single_exit) = stable_run(&dir, "1", extra_args);
        let (multi, multi_exit) = stable_run(&dir, "8", extra_args);

        assert_eq!(
            single_exit, multi_exit,
            "{label}: exit code differs single (jobs=1) vs multi (jobs=8)"
        );
        assert_eq!(
            single, multi,
            "{label}: stdout differs single (jobs=1) vs multi (jobs=8)"
        );
        // Sanity: the fixture actually produced output on every channel
        // that isn't just the two runs both being empty by coincidence.
        assert!(
            !single.is_empty(),
            "{label}: fixture produced no output at all"
        );
    }
}

/// A tree small enough that `check_worker_count` clamps the requested
/// worker count down to the file count (never spawns an empty chunk),
/// still agrees with the forced-parallel run.
#[test]
fn worker_count_above_file_count_still_matches() {
    let dir = tmpdir("mt_determinism_few_files");
    std::fs::write(dir.join("only.rb"), "Missing::Thing\n").unwrap();

    let (single, single_exit) = stable_run(&dir, "1", &[]);
    let (multi, multi_exit) = stable_run(&dir, "64", &[]);

    assert_eq!(single_exit, multi_exit);
    assert_eq!(single, multi);
}
