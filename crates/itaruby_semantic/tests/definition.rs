//! Unit tests for `definition_at`: three call-site kinds that answer, three
//! that must stay silent (`None`) per invariant #1 extended to navigation —
//! no wrong answers, ever.

fn db_with_fixture(name: &str) -> (itaruby_semantic::Db, itaruby_semantic::SourceFile, String) {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata");
    let path = format!("{dir}/{name}");
    let text = std::fs::read_to_string(&path).unwrap();
    let db = itaruby_semantic::Db::default();
    let file = itaruby_semantic::SourceFile::new(&db, path.into(), text.clone());
    itaruby_semantic::ProjectFiles::new(&db, vec![file]);
    (db, file, text)
}

/// Byte offset of `ident` where it's reached via the unique `needle` prefix
/// (e.g. `needle = "Widget2.new.spin"`, `ident = "spin"`): avoids matching
/// an unrelated earlier occurrence of the bare identifier (a `def`, or a
/// substring inside another word).
fn offset_in(text: &str, needle: &str, ident: &str) -> usize {
    let start = text.find(needle).unwrap_or_else(|| panic!("`{needle}` not found in fixture"));
    start + needle.rfind(ident).expect("ident must be a suffix of needle")
}

fn def_line(db: &itaruby_semantic::Db, site: itaruby_semantic::DefSite) -> (String, u32) {
    let text = site.file.text(db);
    let (line, _) = itaruby_semantic::LineIndex::new(text).line_col(text, site.start);
    (site.file.path(db).display().to_string(), line + 1)
}

#[test]
fn instance_method_call_resolves() {
    let (db, file, text) = db_with_fixture("definition_ok.rb");
    let offset = offset_in(&text, "Widget2.new.spin", "spin");
    let site = itaruby_semantic::definition_at(&db, file, offset)
        .expect("Widget2.new.spin must resolve to `def spin`");
    let (path, line) = def_line(&db, site);
    assert!(path.ends_with("definition_ok.rb"), "wrong file: {path}");
    assert_eq!(line, 6, "`def spin` is on line 6");
}

#[test]
fn singleton_method_call_resolves() {
    let (db, file, text) = db_with_fixture("definition_ok.rb");
    let offset = offset_in(&text, "Widget2.build", "build");
    let site = itaruby_semantic::definition_at(&db, file, offset)
        .expect("Widget2.build must resolve to `def self.build`");
    let (path, line) = def_line(&db, site);
    assert!(path.ends_with("definition_ok.rb"), "wrong file: {path}");
    assert_eq!(line, 2, "`def self.build` is on line 2");
}

#[test]
fn implicit_self_call_resolves() {
    let (db, file, text) = db_with_fixture("definition_ok.rb");
    let offset = offset_in(&text, "\n    helper\n", "helper");
    let site = itaruby_semantic::definition_at(&db, file, offset)
        .expect("implicit-self `helper` call must resolve to `def helper`");
    let (path, line) = def_line(&db, site);
    assert!(path.ends_with("definition_ok.rb"), "wrong file: {path}");
    assert_eq!(line, 10, "`def helper` is on line 10");
}

#[test]
fn open_class_call_is_silent() {
    let (db, file, text) = db_with_fixture("open_class.rb");
    let offset = offset_in(&text, "nonexistent_method", "nonexistent_method");
    assert!(
        itaruby_semantic::definition_at(&db, file, offset).is_none(),
        "method_missing makes Widget open: no definition may be guessed"
    );
}

#[test]
fn external_gem_call_is_silent() {
    let (db, file, text) = db_with_fixture("definition_silent.rb");
    let offset = offset_in(&text, ".fetch", "fetch");
    assert!(
        itaruby_semantic::definition_at(&db, file, offset).is_none(),
        "receiver is an unresolvable external constant: must stay silent"
    );
}

#[test]
fn incomplete_ancestry_call_is_silent() {
    let (db, file, text) = db_with_fixture("definition_silent.rb");
    let offset = offset_in(&text, "\n    helper_from_base\n", "helper_from_base");
    assert!(
        itaruby_semantic::definition_at(&db, file, offset).is_none(),
        "superclass ExternalBase is unresolvable: ancestry incomplete, must stay silent"
    );
}
