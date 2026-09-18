//! The abstract-raise idiom (`raise NotImplementedError` in a base
//! method) means "subclasses complete me" — a template-method contract —
//! not "every lookup on the family goes silent". The base class is no
//! longer blanket-open for instance lookups: its own methods resolve, and
//! a `NotFound` on the family softens to `Inconclusive` only when a
//! structural descendant PROVES the method name (`abstract_family_defines`,
//! the name-keyed counterpart of `descendant_defines`). Fail-closed: an
//! open descendant (any other reason), a `method_missing` member, or an
//! incomplete/open chain under the base keeps the whole family silent.
//!
//! This is the mechanism that hid the real production `NameError` behind
//! rails dd1c8848 (`actionview/lib/action_view/helpers/tags/
//! search_field.rb:12`, bare `request` on `Tags::SearchField` — the fix
//! was `@template_object.request`). On the PRISTINE rails tree the
//! diagnostic still does not fire: `Tags::SearchField`'s own ancestry
//! carries open helper modules (`FormTagHelper`'s `mattr_accessor`), so
//! the lookup is Inconclusive before any abstract-raise logic applies —
//! firing there would require ignoring open ancestors, which invariant #1
//! forbids.
//!
//! Fixtures under `testdata/abstract_raise/`. Two-sided proof: the
//! accusing fixtures re-emit their exact diagnostics under the fix and go
//! silent again when the lookup hunk is reverted (mutant run, 2026-09-03).

fn check_fixture(name: &str) -> Vec<String> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/abstract_raise");
    let path = format!("{dir}/{name}");
    let text = std::fs::read_to_string(&path).unwrap();
    let db = itaruby_semantic::Db::default();
    let file = itaruby_semantic::SourceFile::new(&db, path.into(), text.clone());
    itaruby_semantic::ProjectFiles::new(&db, vec![file]);
    let li = itaruby_semantic::LineIndex::new(&text);
    itaruby_semantic::check_file(&db, file)
        .iter()
        .map(|d| {
            let (l, c) = li.line_col(&text, d.start);
            format!("{}:{}:{} {}", l + 1, c + 1, d.code, d.message)
        })
        .collect()
}

/// The bug: the base's own `draw` self-sends `request`, no member of the
/// family (base or either closed descendant) defines it — exactly one
/// E0101, at the call, naming the method and the base class. Before the
/// fix this was silent: the base was blanket-open.
#[test]
fn undefined_name_on_abstract_raise_family_accuses() {
    let diags = check_fixture("base_calls_undefined_name_accuses.rb");
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(
        diags[0].starts_with("15:5:E0101"),
        "expected the exact historical shape at 15:5, got: {:?}",
        diags[0]
    );
    assert!(
        diags[0].contains("`request`"),
        "message should name the missing method, got: {:?}",
        diags[0]
    );
    assert!(
        diags[0].contains("AbstractRaiseAccuseBase"),
        "message should name the receiver class, got: {:?}",
        diags[0]
    );
}

/// A descendant's closed MIXIN can supply the hook: the family walk reads
/// each member's full chain (includes included) for the name, so a
/// definition arriving via `include` keeps the family silent — the control
/// that keeps the name-keyed walk from inventing an E0101.
#[test]
fn descendant_mixin_defines_name_stays_silent() {
    let diags = check_fixture("descendant_mixin_defines_stays_silent.rb");
    assert!(
        diags.is_empty(),
        "`request` is supplied by a module the descendant includes: no E0101. Got: {diags:?}"
    );
}

/// The discourse shape (endpoints/base.rb:280, 5 measured sites): a base
/// stub overridden by EVERY descendant with a different signature. The
/// call in the base's own body dispatches on the descendant instance, so
/// the stub's arity never runs — an E0102 there was a false positive.
#[test]
fn stub_shadowed_by_descendant_override_is_silent() {
    let diags = check_fixture("stub_shadowed_by_descendant_silent.rb");
    assert!(
        diags.is_empty(),
        "every descendant overrides the stub with 3 params: no E0102. Got: {diags:?}"
    );
}

/// The other side: with NO descendant overriding the name, the stub is
/// the effective method and a wrong-arity call to it is a real latent
/// `ArgumentError` — the check must survive.
#[test]
fn unshadowed_stub_wrong_arity_still_reports() {
    let diags = check_fixture("stub_unshadowed_wrong_arity_accuses.rb");
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(
        diags[0].starts_with("11:5:E0102"),
        "expected E0102 at the 3-arg call, got: {:?}",
        diags[0]
    );
    assert!(
        diags[0].contains("`prepare`"),
        "message should name the stub, got: {:?}",
        diags[0]
    );
}

/// The scope control (the rails `Tags::SearchField` shape): a SIBLING
/// descendant's open mixin must not silence a lookup it can never answer
/// — the runtime dispatch set of a bare self-send is the RECEIVER's
/// subtree, so the walk starts there, not at the abstract ancestor.
/// Walking the whole ancestor family silenced the real rails `request`
/// `NameError` (`Tags::ActionText` pulls in an open `FormTagHelper`).
#[test]
fn sibling_open_mixin_does_not_silence_receiver_dispatch() {
    let diags = check_fixture("sibling_open_mixin_still_accuses.rb");
    assert_eq!(diags.len(), 1, "expected exactly 1 diagnostic, got: {diags:?}");
    assert!(
        diags[0].starts_with("24:5:E0101"),
        "expected E0101 at the receiver's own call, got: {:?}",
        diags[0]
    );
}

/// The softening the mechanism exists for: one descendant DOES define
/// `request`, so the base's self-send is a template-method hook supplied
/// at runtime. Silent.
#[test]
fn descendant_defines_name_stays_silent() {
    let diags = check_fixture("descendant_defines_stays_silent.rb");
    assert!(
        diags.is_empty(),
        "`request` is defined by a descendant: no E0101. Got: {diags:?}"
    );
}

/// Fail-closed refinement (a): a descendant open for any other reason —
/// here `method_missing` — could define anything, so the family stays
/// Inconclusive even though no descendant literally defines `request`.
#[test]
fn open_descendant_keeps_family_silent() {
    let diags = check_fixture("open_descendant_stays_silent.rb");
    assert!(
        diags.is_empty(),
        "a method_missing descendant could define anything: no E0101. Got: {diags:?}"
    );
}

/// Fail-closed refinement (b): the base's superclass is a curated
/// `DeclaredExternal` constant (`ActiveRecord::Base`), which reads as
/// still-unknown — the family's surface is provably incomplete, every
/// lookup on it stays Inconclusive, and no E0104 fires (the name
/// resolves through the declarations file).
#[test]
fn declared_external_superclass_keeps_family_silent() {
    let diags = check_fixture("external_super_stays_silent.rb");
    assert!(
        diags.is_empty(),
        "a DeclaredExternal superclass reads as still-unknown: no E0101, no E0104. Got: {diags:?}"
    );
}

/// The abstract method itself resolves: `render` IS defined (its body
/// raises), so a self-send of it from the base's own body is Found.
#[test]
fn abstract_method_itself_resolves_silently() {
    let diags = check_fixture("abstract_method_itself_silent.rb");
    assert!(
        diags.is_empty(),
        "`render` is defined on the base itself: no E0101. Got: {diags:?}"
    );
}
