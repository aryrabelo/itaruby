//! End-to-end CLI tests for `db/structure.sql` as a type declaration
//! source (bead ita-muf): real binary, real checked-in fixtures under
//! `testdata/`, no library shortcuts — mirrors `definition_cli.rs`'s style.
//!
//! `ApplicationRecord` has no project definition anywhere under
//! `testdata/`, exactly like `schema_attrs.rs`'s fixtures (bead ita-yho):
//! every scenario therefore also emits a `warning[E0104]: unresolved
//! constant ApplicationRecord` line, orthogonal pre-existing noise this
//! bead doesn't touch. `without_e0104` strips it so "silent" assertions
//! check the thing this bead actually owns.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn testdata_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata")
}

fn run_check(arg: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ita"))
        .arg("check")
        .arg(arg)
        .output()
        .expect("run `ita check`")
}

/// Strips whole E0104 diagnostics: primary line plus the excerpt/caret
/// block that follows it (w12 closure made every diagnostic multi-line,
/// so filtering line-by-line would leak excerpt lines through).
fn without_e0104(out: &Output) -> String {
    let text = String::from_utf8_lossy(&out.stdout);
    let mut kept: Vec<&str> = Vec::new();
    let mut in_e0104_block = false;
    for line in text.lines() {
        if line.contains(": error[") || line.contains(": warning[") {
            in_e0104_block = line.contains("[E0104]");
            if in_e0104_block {
                continue;
            }
        } else if in_e0104_block {
            continue;
        }
        kept.push(line);
    }
    kept.join("\n")
}

#[test]
fn valid_attr_is_silent() {
    let out = run_check(&testdata_root().join("structure_sql_models/valid_attr.rb"));
    let s = without_e0104(&out);
    assert!(s.trim().is_empty(), "expected silence, got: {s}");
}

#[test]
fn non_numeric_string_into_integer_column_warns_e0106() {
    let out = run_check(&testdata_root().join("structure_sql_models/bad_integer_literal.rb"));
    let s = without_e0104(&out);
    assert!(s.contains("warning[E0106]"), "expected E0106, got: {s}");
    assert!(s.contains("`quantity`"), "expected column name in message: {s}");
}

#[test]
fn numeric_string_into_integer_column_is_silent() {
    let out = run_check(&testdata_root().join("structure_sql_models/numeric_string_ok.rb"));
    let s = without_e0104(&out);
    assert!(s.trim().is_empty(), "\"42\" must survive Rails' cast, got: {s}");
}

#[test]
fn string_into_character_varying_column_is_always_silent() {
    let out = run_check(&testdata_root().join("structure_sql_models/varchar_string_silent.rb"));
    let s = without_e0104(&out);
    assert!(s.trim().is_empty(), "String columns never risk E0106, got: {s}");
}

#[test]
fn string_into_numeric_column_is_silent_v1_gap() {
    let out = run_check(&testdata_root().join("structure_sql_models/numeric_column_silent.rb"));
    let s = without_e0104(&out);
    assert!(
        s.trim().is_empty(),
        "`numeric` maps to Ty::Unknown in v1 (accepted false negative), got: {s}"
    );
}

#[test]
fn unknown_attribute_is_silent() {
    let out = run_check(&testdata_root().join("structure_sql_models/unknown_attr.rb"));
    let s = without_e0104(&out);
    assert!(
        s.trim().is_empty(),
        "incomplete ApplicationRecord ancestry keeps this Inconclusive, not E0101, got: {s}"
    );
}

#[test]
fn model_without_a_matching_table_is_silent() {
    let out = run_check(&testdata_root().join("structure_sql_models/missing_table.rb"));
    let s = without_e0104(&out);
    assert!(s.trim().is_empty(), "expected silence, got: {s}");
}

#[test]
fn structure_sql_itself_is_never_diagnosed() {
    let out = run_check(&testdata_root().join("structure_sql_models"));
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(!s.contains("structure.sql"), "the dump must never be checked as code: {s}");
}

#[test]
fn schema_rb_wins_over_structure_sql_when_both_exist() {
    let out = run_check(&testdata_root().join("structure_sql_precedence/model.rb"));
    let s = without_e0104(&out);
    // db/schema.rb declares `quantity` as `integer` (E0106 risk);
    // db/structure.sql in the same directory declares it `text` (no risk).
    // E0106 firing is only possible if schema.rb was the source used.
    assert!(s.contains("warning[E0106]"), "schema.rb must win when both exist, got: {s}");
    let sql = String::from_utf8_lossy(&out.stdout);
    assert!(!sql.contains("structure.sql"), "structure.sql must stay unread, got: {sql}");
}
