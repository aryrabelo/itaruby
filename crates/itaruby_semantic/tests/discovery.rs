//! Unit/integration tests for `itaruby_semantic::discovery::wire_declaration_sources`
//! (bead ita-16g): the shared upward search + salsa wiring for `db/schema.rb`,
//! `db/structure.sql`, and a client's `sorbet/rbi`, extracted out of
//! `crates/itaruby/src/main.rs` so `ita check` and `ita server` run the exact
//! same discovery. Real tempdirs (`std::env::temp_dir()`), never `testdata/`
//! — this exercises the upward filesystem walk itself, not a fixed layout.

use std::path::{Path, PathBuf};

use itaruby_semantic::{
    wire_declaration_sources, Db, RbiCoreReopenings, RbiProject, StructureSqlProject,
};

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("itaruby-discovery-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// `db/schema.rb` sitting 2 directory levels above the checked root must be
/// found and returned as a declaration-only source — the same 4-level
/// upward walk `ita check`'s old (now moved) `find_upward` did.
#[test]
fn finds_schema_rb_two_levels_above_root() {
    let dir = tempdir("schema-two-up");
    write(&dir.join("db/schema.rb"), "ActiveRecord::Schema.define do\nend\n");
    let root = dir.join("app/models");
    std::fs::create_dir_all(&root).unwrap();

    let mut db = Db::new();
    let discovered = wire_declaration_sources(&mut db, &[root]);

    assert_eq!(discovered.schema_rb, Some(dir.join("db/schema.rb")));
    assert_eq!(discovered.declarations_only, vec![dir.join("db/schema.rb")]);
    assert_eq!(
        discovered.structure_sql, None,
        "schema.rb found: structure.sql must not even be looked up"
    );
    assert!(
        StructureSqlProject::try_get(&db).is_none(),
        "structure.sql must never be wired when schema.rb wins"
    );
}

/// `db/schema.rb` and `db/structure.sql` both present: schema.rb wins,
/// structure.sql is never even searched for, let alone wired — the two are
/// never merged, never mixed.
#[test]
fn schema_rb_wins_over_structure_sql_when_both_exist() {
    let dir = tempdir("precedence");
    write(&dir.join("db/schema.rb"), "ActiveRecord::Schema.define do\nend\n");
    write(&dir.join("db/structure.sql"), "CREATE TABLE widgets (id integer);\n");

    let mut db = Db::new();
    let discovered = wire_declaration_sources(&mut db, std::slice::from_ref(&dir));

    assert_eq!(discovered.schema_rb, Some(dir.join("db/schema.rb")));
    assert_eq!(discovered.structure_sql, None);
    assert!(StructureSqlProject::try_get(&db).is_none());
}

/// No `db/schema.rb` anywhere upward: `db/structure.sql` is discovered and
/// wired into `StructureSqlProject` instead — never through `ProjectFiles`.
#[test]
fn falls_back_to_structure_sql_when_no_schema_rb() {
    let dir = tempdir("structure-fallback");
    write(&dir.join("db/structure.sql"), "CREATE TABLE widgets (id integer);\n");

    let mut db = Db::new();
    let discovered = wire_declaration_sources(&mut db, std::slice::from_ref(&dir));

    assert_eq!(discovered.schema_rb, None);
    assert!(discovered.declarations_only.is_empty());
    assert_eq!(discovered.structure_sql, Some(dir.join("db/structure.sql")));
    let wired = StructureSqlProject::try_get(&db).expect("structure.sql must be wired");
    assert_eq!(wired.file(&db).path(&db), &dir.join("db/structure.sql"));
}

/// `sorbet/rbi` discovered upward wires both `RbiProject` (constants) and
/// `RbiCoreReopenings` (core namespace reopenings) — bead ita-vto/w12's
/// phase-1 scan, run from the shared discovery module.
#[test]
fn finds_and_wires_sorbet_rbi() {
    let dir = tempdir("rbi");
    write(
        &dir.join("sorbet/rbi/gems/discotk.rbi"),
        "# typed: true\n\nclass DiscoTk::Widget\nend\n\nclass String\n  def discofy; end\nend\n",
    );
    let root = dir.join("app");
    std::fs::create_dir_all(&root).unwrap();

    let mut db = Db::new();
    let discovered = wire_declaration_sources(&mut db, &[root]);

    assert_eq!(discovered.rbi_dir, Some(dir.join("sorbet/rbi")));
    let rbi = RbiProject::try_get(&db).expect("sorbet/rbi must wire RbiProject");
    assert!(rbi.map(&db).contains_key("DiscoTk::Widget"));
    let reopenings =
        RbiCoreReopenings::try_get(&db).expect("core reopening must wire RbiCoreReopenings");
    assert!(reopenings.map(&db).contains_key("String"));
}

/// Nothing to find anywhere upward: every field stays empty/`None`, and no
/// salsa singleton is ever wired — invariant #1's silence, at the
/// discovery layer.
#[test]
fn nothing_found_is_a_fully_empty_report() {
    let dir = tempdir("nothing");
    let root = dir.join("app/models");
    std::fs::create_dir_all(&root).unwrap();

    let mut db = Db::new();
    let discovered = wire_declaration_sources(&mut db, &[root]);

    assert_eq!(discovered.schema_rb, None);
    assert_eq!(discovered.structure_sql, None);
    assert_eq!(discovered.rbi_dir, None);
    assert!(discovered.declarations_only.is_empty());
    assert!(StructureSqlProject::try_get(&db).is_none());
    assert!(RbiProject::try_get(&db).is_none());
    assert!(RbiCoreReopenings::try_get(&db).is_none());
}

/// Discovery through a symlinked root must find the real project's
/// `db/schema.rb` — the link's own lexical parents don't contain it. This
/// is the silence half of the 2026-08-24 finding: reading that absence as
/// "no declarations" is the same shape that read a missing `Gemfile` as
/// "no gems" and fabricated 310 diagnostics.
#[cfg(unix)]
#[test]
fn symlinked_root_finds_the_real_schema() {
    let dir = tempdir("symlink-finds");
    write(&dir.join("proj").join("db").join("schema.rb"), "");
    std::fs::create_dir_all(dir.join("proj").join("app")).unwrap();
    let link = dir.join("linked-app");
    std::os::unix::fs::symlink(dir.join("proj").join("app"), &link).unwrap();

    let mut db = Db::new();
    let discovered = wire_declaration_sources(&mut db, &[link]);

    assert_eq!(
        discovered.declarations_only.len(),
        1,
        "a symlinked root must resolve to its real project: {:?}",
        discovered.declarations_only
    );
}

/// The other half, and the one a machine-dependent tempdir will not catch:
/// when the caller's OWN form of a directory holds the file, that is the
/// form handed back. Every consumer membership-tests these paths against
/// paths it already holds — LSP document paths, `discover_rb_files` output
/// — so returning a canonicalized path where nothing else is canonicalized
/// silently stops matching, and a `db/schema.rb` opened in the editor
/// starts receiving diagnostics it is exempt from.
///
/// The fixture makes the two forms provably different by putting the whole
/// project behind a symlinked *parent*, so the caller's path and its
/// resolved form share no prefix.
#[cfg(unix)]
#[test]
fn discovered_path_stays_in_the_callers_own_space() {
    let dir = tempdir("symlink-space");
    let real = dir.join("real");
    write(&real.join("db").join("schema.rb"), "");
    std::fs::create_dir_all(real.join("app")).unwrap();
    let via = dir.join("via");
    std::os::unix::fs::symlink(&real, &via).unwrap();

    let root = via.join("app");
    let mut db = Db::new();
    let discovered = wire_declaration_sources(&mut db, &[root]);

    let found = discovered.schema_rb.expect("schema found through the link");
    assert!(
        found.starts_with(&via),
        "expected the caller's own form under {}, got {}",
        via.display(),
        found.display()
    );
}
