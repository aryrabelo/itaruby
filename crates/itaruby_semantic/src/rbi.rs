//! Tapioca-generated `sorbet/rbi/**/*.rbi` as a lazily-loaded external
//! declaration source (bead ita-vto) — the RBI analogue of `declarations.rs`
//! (bead ita-3gs)'s hand-curated `gems.rbi`, except sourced from a client
//! project's own generated RBI tree instead of a small file this repo
//! ships. Two phases, matching the bead's scale constraint (2.3M lines /
//! ~1900 files at the reference corpus can never be parsed per check):
//!
//! Phase 1 (`build_rbi_map`, this file): a cheap line scan, no prism, one
//! file's contents in memory at a time (`BufRead::lines`, never the whole
//! tree at once) — builds `constant name -> every declaring file` from
//! every `class NAME`/`module NAME` header it sees, at ANY indentation
//! (bead ita-k9j.3: EVERY declaring file, never just one — see
//! `RbiIndex`'s doc comment for why first-wins was a bug, not a
//! simplification).
//!
//! The name that map is keyed on is NOT always fully qualified, and that
//! is the whole caveat of this file (bead ita-iyz, measured against the
//! reference corpus's real RBI tree). Under `gems/`, Tapioca does write
//! the fully qualified constant on one line (`class Foo::Bar::Baz`), so
//! the key is the full path. Under `dsl/` it does NOT: a column-0 `class
//! Model` nests indented `module GeneratedAttributeMethods`, `module
//! GeneratedAssociationMethods`, `class PrivateRelation` and friends, and
//! `scan_class_name` trims leading space, so those land in the map under
//! their BARE name. Since ~1587 dsl files reuse the same generated names,
//! those keys collide and the surviving file is arbitrary.
//!
//! So: this map is a coarse file LOCATOR (now a SET of locators per name,
//! bead ita-k9j.3), never the source of truth for a resolved name. Every
//! consumer MUST refilter each file it points at against the real
//! qualified fragment path, computed by the prism parse in phase 2 —
//! which does track nesting. Today all of them do (`index.rs`'s
//! `rbi_declares`, `rbi_ancestor_closure`, `resolve_method_node`), so a
//! bare-name collision degrades to a silent miss, never to a wrong type;
//! `tests/dsl_rbi.rs::nested_module_precedence_stays_per_file` locks
//! that. A new consumer that trusts the map directly would silently
//! attribute one model's methods to another. This is an index, not
//! semantics — same discipline as `structure_sql.rs`'s scanner.
//!
//! Phase 2 lives in `index.rs::merge_rbi_declarations`: parses, with the
//! exact same `parse_defs_text` every project file already uses, only the
//! one file that declares a constant the project's own code actually
//! references and could not otherwise resolve. See that function's doc
//! comment for the merge contract.

use std::collections::HashMap;
use std::io::BufRead;
use std::path::{Path, PathBuf};

/// Every `.rbi` file under `root`, recursively, SORTED by path. Stdlib-only
/// walk (no gitignore semantics needed for a client's generated RBI tree,
/// unlike `crates/itaruby/src/main.rs::discover_rb_files`'s `ignore` crate
/// use for real project sources) — never panics on an unreadable
/// directory, just yields fewer files. The sort (bead ita-k9j.3) is
/// reproducibility only, NOT the correctness fix: `read_dir` order is
/// arbitrary and OS/filesystem-dependent, so without a sort the exact same
/// tree can enumerate differently between runs or machines, making any
/// observed behavior change look mysterious rather than attributable to a
/// real content change. `build_rbi_index`'s UNION of every declaring file
/// is the actual fix — see that function's and `RbiIndex`'s doc comments.
pub fn discover_rbi_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    walk(root, &mut out);
    out.sort();
    out
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rbi") {
            out.push(path);
        }
    }
}

/// The constant a `class`/`module` header line declares, exactly as the
/// line spells it: `class Foo::Bar::Baz` / `class Foo::Bar < Baz` /
/// `module Foo` / `class Foo # comment`. Not a declaration line
/// (`class << self`, a stray `end`, a comment, blank) — never a phantom
/// name, same discipline as `structure_sql.rs`'s column scanner.
///
/// Leading whitespace is trimmed, so an INDENTED header (Tapioca's `dsl/`
/// nests them) yields its bare, unqualified name — `GeneratedAttribute
/// Methods`, not `Model::GeneratedAttributeMethods`. See this module's
/// doc comment: that is why the map it feeds is a file locator only.
pub fn scan_class_name(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    let rest = trimmed
        .strip_prefix("class ")
        .or_else(|| trimmed.strip_prefix("module "))?
        .trim_start();
    let end = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == ':'))
        .unwrap_or(rest.len());
    let name = &rest[..end];
    if name.starts_with(|c: char| c.is_ascii_uppercase()) {
        Some(name)
    } else {
        None
    }
}

/// What phase 1 harvests from one RBI tree: the `constant name -> EVERY
/// declaring file` map (bead ita-k9j.3: exhaustive, never first-wins — a
/// stub reopening (`class ActiveRecord::Base; include Flipper::Adapters
/// ::ActiveRecord; end`) IS a real reopening, exactly like `core_reopenings`
/// below already treats a core-namespace reopening: at runtime both
/// declarations contribute to the class's actual ancestry, so dropping
/// either one is a fabricated fact, not a simplification. Measured on a
/// real Tapioca corpus: three files declare `class ActiveRecord::Base` —
/// the real `activerecord@*.rbi` (123 direct edges) plus two reopening
/// stubs, `activestorage@*.rbi` (8 edges) and `flipper@*.rbi` (5 edges).
/// Before this fix, `constants` kept only ONE of the three — whichever
/// `discover_rbi_files` happened to enumerate first, an order the
/// filesystem never guarantees — so every consumer's RBI closure walk
/// silently started from 5 or 8 edges instead of 123 whenever a stub won
/// the race. Every consumer (`index.rs`'s `rbi_declares`,
/// `rbi_ancestor_closure`, `resolve_method_node`) now walks the UNION of
/// every file in the vector — same discipline `core_reopenings` already
/// used, and its own doc comment explains why: "first-wins would
/// silently drop the second gem that also reopens `String`". This
/// struct's second field, separate from `constants`, is EVERY file that
/// reopens a core namespace (`String`, `Kernel`, ... —
/// `core::is_core_namespace`), all occurrences kept: gem RBIs are how
/// `ActiveSupport`'s `String#squish` reaches a core receiver, and a
/// first-wins map there would silently drop the second gem that also
/// reopens `String` (a missed declaration would mean a false E0101, so
/// that map must be exhaustive too).
#[derive(Default)]
pub struct RbiIndex {
    pub constants: HashMap<String, Vec<PathBuf>>,
    pub core_reopenings: HashMap<String, Vec<PathBuf>>,
}

/// Phase 1 scan. One file's contents in memory at a time
/// (`BufRead::lines`), never the whole tree; a single unreadable line or
/// file is skipped, never aborts the scan.
pub fn build_rbi_index(files: &[PathBuf]) -> RbiIndex {
    let mut index = RbiIndex::default();
    for path in files {
        let Ok(f) = std::fs::File::open(path) else { continue };
        for line in std::io::BufReader::new(f).lines() {
            let Ok(line) = line else { continue };
            if let Some(name) = scan_class_name(&line) {
                // bead ita-k9j.3: every declaring file, not just the
                // first — same `.entry(..).or_default().push(..)` shape
                // `core_reopenings` already used below. `files` is sorted
                // by `discover_rbi_files`, so the vector's order (and
                // therefore which file wins a same-name method conflict
                // downstream) is deterministic, not filesystem luck.
                index.constants.entry(name.to_string()).or_default().push(path.clone());
                if crate::core::is_core_namespace(name) {
                    index
                        .core_reopenings
                        .entry(name.to_string())
                        .or_default()
                        .push(path.clone());
                }
            }
        }
    }
    index
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_fully_qualified_class_and_module_headers() {
        assert_eq!(scan_class_name("class Foo::Bar::Baz"), Some("Foo::Bar::Baz"));
        assert_eq!(scan_class_name("  module Foo"), Some("Foo"));
        assert_eq!(scan_class_name("class Foo::Bar < Baz"), Some("Foo::Bar"));
        assert_eq!(scan_class_name("class Foo # comment"), Some("Foo"));
    }

    #[test]
    fn ignores_non_declaration_lines() {
        assert_eq!(scan_class_name("class << self"), None);
        assert_eq!(scan_class_name("  end"), None);
        assert_eq!(scan_class_name("# class Foo"), None);
        assert_eq!(scan_class_name(""), None);
        assert_eq!(scan_class_name("classify(foo)"), None);
    }

    #[test]
    fn build_rbi_index_reads_only_declared_files() {
        let dir = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../testdata/tapioca_rbi/sorbet/rbi"
        );
        let files = discover_rbi_files(Path::new(dir));
        assert!(!files.is_empty(), "fixture must carry at least one .rbi file");
        let index = build_rbi_index(&files);
        assert!(
            index.constants.contains_key("TapiocaVtoFixtureGem::Widget"),
            "expected fixture to declare TapiocaVtoFixtureGem::Widget, got: {:?}",
            index.constants.keys().collect::<Vec<_>>()
        );
        assert!(index.constants.contains_key("TapiocaVtoFixtureGem::Base"));
    }

    #[test]
    fn core_reopenings_keep_every_occurrence() {
        // One file declaring String twice, another reopening it again: the
        // reopening map must be exhaustive (all files), unlike the
        // constants map — a dropped file is a dropped `String#squish`, i.e.
        // a false E0101 under closed-world-via-Tapioca.
        // ponytail: crate target dir, not system temp — same
        // machine-local/wiped-with-clean contract as CARGO_TARGET_TMPDIR,
        // which cargo only sets for integration tests.
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/itaruby-rbi-core-reopen-test");
        std::fs::create_dir_all(&dir).expect("create test rbi dir");
        std::fs::write(dir.join("a.rbi"), "class String\n  def a; end\nend\n\nclass String\n  def a2; end\nend\n").expect("write a.rbi");
        std::fs::write(dir.join("b.rbi"), "class String\n  def b; end\nend\nmodule Kernel\n  def k; end\nend\n").expect("write b.rbi");
        std::fs::write(dir.join("c.rbi"), "class Widget\nend\n").expect("write c.rbi");
        let files = discover_rbi_files(&dir);
        let index = build_rbi_index(&files);
        assert_eq!(
            index.core_reopenings["String"].len(),
            3,
            "two reopenings in a.rbi plus one in b.rbi, got: {:?}",
            index.core_reopenings
        );
        assert_eq!(index.core_reopenings["Kernel"].len(), 1);
        assert!(!index.core_reopenings.contains_key("Widget"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
