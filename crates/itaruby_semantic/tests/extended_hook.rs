//! Bead H, onda 2: `def self.extended(base)`.
//!
//! The hook receives the EXTERNAL object — the class doing the `extend` —
//! as a parameter, and installs methods on it: `ActiveModel::Naming`
//! calls `base.delegate :model_name, to: :class`
//! (`activemodel/lib/active_model/naming.rb:263-266`), which installs
//! `model_name` as an INSTANCE method of `Blog::Post`
//! (`activemodel/test/models/blog_post.rb:9`). `extend M` alone only reads
//! `M`'s own instance methods onto the extender's SINGLETON track
//! (`ProjectIndex::lookup_singleton`), so nothing in this index reached
//! the install the hook performs on the extender's instance surface, and
//! `Blog::Post.new.model_name` — code that runs — was one of rails'
//! baseline errors (`activemodel/test/cases/naming_test.rb:333`).
//!
//! `apply_extended_hooks` files what the hook installs where it really
//! lands, and EVERY fixture here was run under MRI: the six
//! `*_resolves_silently`/`*_opens_the_extender` fixtures exit 0, and each
//! accusing fixture raises `NoMethodError` on the exact line `ita check`
//! blames.
//!
//! Both sides are pinned per shape, because a shape silently dropped is a
//! false positive waiting for the next corpus:
//!
//! * READ — `delegate` (with a delegated method's real arity, which is
//!   why the install is arity-inert), `define_method`, a literal
//!   `class_eval do ... end` block.
//! * THE TARGET SURFACE — `def base.x` lands on the base OBJECT's
//!   singleton, so `User.new.pi` must keep accusing.
//! * KEYING — a hook-less module's extender, a class that never extends,
//!   and a call on a local that is NOT the hook's parameter all keep
//!   accusing.
//! * FAIL-CLOSED — a hook that installs through `send`, `instance_eval`,
//!   a string `class_eval`, a dynamic `define_method`, a splat
//!   `delegate`, or a name-rewriting `delegate` keyword names methods
//!   this AST cannot read, so the extender's surface is open
//!   (`Inconclusive`), never conclusively `NotFound`.
//!
//! The READ arm is deliberately not a blanket open, and one fixture pins
//! that difference: `delegate_hook_resolves_silently.rb` calls
//! `never_installed` beside the delegated `model_name` and requires it to
//! be blamed — a hook that names its methods must leave the rest of the
//! surface exactly as closed as it found it.

use itaruby_semantic::{Db, ProjectFiles, SourceFile};

/// One fixture as its own single-file project — the same helper shape
/// `tests/included_hook.rs` uses. Errors only: warnings are the
/// external-declaration gap and say nothing about this bead.
fn errors(name: &str) -> Vec<String> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/extended_hook");
    let path = format!("{dir}/{name}");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    let db = Db::default();
    let file = SourceFile::new(&db, path.into(), text);
    ProjectFiles::new(&db, vec![file]);
    itaruby_semantic::check_file(&db, file)
        .iter()
        .filter(|d| d.severity == itaruby_semantic::Severity::Error)
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect()
}

#[test]
fn delegate_hook_install_is_exactly_the_named_method() {
    let diags = errors("delegate_hook_resolves_silently.rb");
    assert_eq!(diags.len(), 1, "the hook enumerates its install; nothing else leaves the surface: {diags:?}");
    assert!(
        diags[0].contains("E0101") && diags[0].contains("`never_installed`") && diags[0].contains("ExtHookNamingPost"),
        "expected the one error to blame `never_installed`, never the delegated `model_name`: {diags:?}"
    );
}

#[test]
fn define_method_hook_install_resolves_silently() {
    let diags = errors("define_method_hook_resolves_silently.rb");
    assert!(diags.is_empty(), "the hook's `base.define_method(:greet)` installs an instance method: {diags:?}");
}

#[test]
fn class_eval_block_hook_install_resolves_silently() {
    let diags = errors("class_eval_block_hook_resolves_silently.rb");
    assert!(diags.is_empty(), "a bare `def` in the hook's literal `base.class_eval` block is an instance method: {diags:?}");
}

#[test]
fn singleton_def_hook_keeps_the_instance_surface_closed() {
    let diags = errors("singleton_def_hook_keeps_the_instance_surface_closed.rb");
    assert_eq!(diags.len(), 1, "`def base.pi` is a singleton method, not an instance one: {diags:?}");
    assert!(diags[0].contains("E0101"), "expected E0101: {diags:?}");
    assert!(diags[0].contains("`pi`") && diags[0].contains("ExtHookSingletonUser"), "expected the instance call to be blamed: {diags:?}");
}

#[test]
fn hook_installs_reach_only_the_extender() {
    let diags = errors("hook_keying_controls_still_accuse.rb");
    assert_eq!(diags.len(), 2, "a hook-less module's extender and a class that never extends: {diags:?}");
    assert!(diags.iter().any(|d| d.contains("ExtHookKeyedBare")), "the hook-less module's extender must accuse: {diags:?}");
    assert!(diags.iter().any(|d| d.contains("ExtHookKeyedUntouched")), "the class that never extends must accuse: {diags:?}");
}

#[test]
fn foreign_receiver_hook_installs_nothing() {
    let diags = errors("foreign_receiver_hook_installs_nothing.rb");
    assert_eq!(diags.len(), 1, "only a call on the hook's own parameter describes the base: {diags:?}");
    assert!(diags[0].contains("E0101") && diags[0].contains("`ghost`"), "expected E0101 on `ghost`: {diags:?}");
}

#[test]
fn send_hook_opens_the_extender() {
    let diags = errors("send_hook_opens_the_extender.rb");
    assert!(diags.is_empty(), "`base.send(:define_method, name)` names a method at runtime: {diags:?}");
}

#[test]
fn instance_eval_hook_opens_the_extender() {
    let diags = errors("instance_eval_hook_opens_the_extender.rb");
    assert!(diags.is_empty(), "a `define_method` inside `base.instance_eval` reaches the instance surface: {diags:?}");
}

#[test]
fn string_class_eval_hook_opens_the_extender() {
    let diags = errors("string_class_eval_hook_opens_the_extender.rb");
    assert!(diags.is_empty(), "a string `base.class_eval` body is unreadable here: {diags:?}");
}

#[test]
fn dynamic_define_method_hook_opens_the_extender() {
    let diags = errors("dynamic_define_method_hook_opens_the_extender.rb");
    assert!(diags.is_empty(), "`base.define_method(name)` installs a name no AST here can read: {diags:?}");
}

#[test]
fn dynamic_delegate_hook_opens_the_extender() {
    let diags = errors("dynamic_delegate_hook_opens_the_extender.rb");
    assert!(diags.is_empty(), "`base.delegate(*names, to: :class)` installs names no AST here can read: {diags:?}");
}

#[test]
fn prefix_delegate_hook_opens_the_extender() {
    let diags = errors("prefix_delegate_hook_opens_the_extender.rb");
    assert!(diags.is_empty(), "`prefix: true` rewrites the installed name, so this hook installs something unreadable here: {diags:?}");
}