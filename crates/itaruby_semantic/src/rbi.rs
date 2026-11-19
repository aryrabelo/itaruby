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

/// Bead ita-h5s: the constant a BARE top-level value-constant write line
/// declares — `XClient = T.let(...)`, `DRbIdConv = DRb::DRbIdConv`,
/// `MODULES = T.let(T.unsafe(nil), T::Array[T.untyped])` — the shape
/// `scan_class_name` above never sees at all (it only ever matches a
/// `class `/`module ` keyword prefix). Measured against all 8 Sorbet
/// corpora under `corpus-candidates/`: every real project ships this
/// shape either as a hand-written shim (planningalerts'
/// `sorbet/rbi/shims/elasticsearch_client.rbi`:
/// `ElasticSearchClient = T.let(T.unsafe(nil), T.nilable(Elasticsearch
/// ::Client))`) or as Tapioca's own compact-alias rendering under
/// `gems/*.rbi` (`DRbIdConv = DRb::DRbIdConv`, `FileList =
/// Rake::FileList`, ...). A QUALIFIED write (`Foo::Bar = ...`) is
/// deliberately NOT matched here — the identifier scan stops at the
/// first `:`, so `rest` after the name never starts with `=` for one,
/// and that shape is already the separate `qualified_writes`/`consts`
/// channel `rbi_qualified_const_declares` (`index.rs`) owns; this
/// function exists only for the BARE, unqualified shape no channel
/// reads today.
///
/// Same discipline as `scan_class_name`: leading whitespace trimmed (so
/// a const written inside a `class Object`/`module Kernel` block is
/// still caught by this coarse locator — phase 2's real parse,
/// `index.rs::rbi_toplevel_const_declares`, is what actually tells a
/// true top-level write from one nested in an arbitrary class/module
/// body), never a phantom name: `==`/`=~`/`=>` right after the
/// identifier are explicitly excluded so a comparison, pattern match, or
/// hash-rocket line is never mistaken for an assignment. This is a
/// cheap LOCATOR ONLY — a comment (`# FakeConst = 1`) or a string/heredoc
/// body line that merely LOOKS like `NAME = ...` can still produce a
/// false-positive candidate here; `rbi_toplevel_const_declares`'s real
/// Prism parse (`FileDefs::toplevel_consts`) is the source of truth that
/// rejects those, exactly like every other RBI channel's phase 1/phase 2
/// split.
pub fn scan_toplevel_const_write(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    if !trimmed.starts_with(|c: char| c.is_ascii_uppercase()) {
        return None;
    }
    let end = trimmed
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .unwrap_or(trimmed.len());
    let name = &trimmed[..end];
    let rest = trimmed[end..].trim_start();
    let mut chars = rest.chars();
    match chars.next() {
        Some('=') => match chars.next() {
            // `==` (comparison), `=~` (regex match), `=>` (hash rocket,
            // rescue-clause/pattern-match binding) are never assignment.
            Some('=' | '~' | '>') => None,
            _ => Some(name),
        },
        _ => None,
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
/// that map must be exhaustive too). Bead ita-h5s: `constants` also
/// harvests `scan_toplevel_const_write`'s bare value-constant-write hits
/// under the SAME key/union discipline — a value write and a
/// class/module header never collide in practice (Ruby forbids reusing
/// one name as both), and every consumer (`rbi_declares`,
/// `rbi_qualified_const_declares`, `rbi_toplevel_const_declares`) already
/// refilters its own candidates by a file-specific, phase-2-verified
/// shape, so widening this shared locator map costs nothing but an
/// extra candidate file a consumer's own exact check rejects.
#[derive(Default)]
pub struct RbiIndex {
    pub constants: HashMap<String, Vec<PathBuf>>,
    pub core_reopenings: HashMap<String, Vec<PathBuf>>,
    /// W11 (Tapioca open world): every file reopening a namespace on a
    /// core receiver's method chain, keyed by the bare name — the
    /// `core_reopenings` set plus `BasicObject`, and a `::`-rooted header
    /// (`class ::Integer`) counted too. Kept apart from `core_reopenings`
    /// so the older consumers of that map stay byte-identical.
    pub chain_reopenings: HashMap<String, Vec<PathBuf>>,
    /// W11: files declaring an instance `is_a?`/`kind_of?`/`instance_of?`
    /// override or a `method_missing`/`respond_to_missing?` anywhere. Any
    /// such class can pass sorbet-runtime's `is_a?` check for a core type
    /// without being one (see `TapiocaEvidence`).
    pub type_test_files: Vec<PathBuf>,
}

/// W11: the bare chain namespace a `class`/`module` header reopens,
/// `::`-rooted or not (`class ::Integer`, `module Kernel`). Indented
/// headers count too: the parsed fragment's path decides later whether a
/// nested `class Integer` really is the top-level one.
pub fn scan_chain_header(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    let rest = trimmed
        .strip_prefix("class ")
        .or_else(|| trimmed.strip_prefix("module "))?
        .trim_start();
    let rest = rest.strip_prefix("::").unwrap_or(rest);
    let end = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .unwrap_or(rest.len());
    if rest[end..].starts_with("::") {
        return None;
    }
    let name = &rest[..end];
    (crate::core::is_core_namespace(name) || name == "BasicObject").then_some(name)
}

/// W11: does this line define an instance method that lets an object
/// pass `is_a?(Integer)` without being an Integer — a type-test override,
/// or the `method_missing` pair a proxy forwards `is_a?` through?
pub fn is_type_test_def(line: &str) -> bool {
    let Some(rest) = line.trim_start().strip_prefix("def ") else {
        return false;
    };
    let rest = rest.trim_start();
    ["is_a?", "kind_of?", "instance_of?", "method_missing", "respond_to_missing?"]
        .iter()
        .any(|name| {
            rest.strip_prefix(name)
                .is_some_and(|tail| tail.is_empty() || tail.starts_with(['(', ' ', ';']))
        })
}

/// Phase 1 scan. One file's contents in memory at a time
/// (`BufRead::lines`), never the whole tree; a single unreadable line or
/// file is skipped, never aborts the scan.
pub fn build_rbi_index(files: &[PathBuf]) -> RbiIndex {
    let mut index = RbiIndex::default();
    for path in files {
        let Ok(f) = std::fs::File::open(path) else { continue };
        let mut type_test = false;
        for line in std::io::BufReader::new(f).lines() {
            let Ok(line) = line else { continue };
            let name = scan_class_name(&line).or_else(|| scan_toplevel_const_write(&line));
            if let Some(name) = name {
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
            if let Some(name) = scan_chain_header(&line) {
                let files = index.chain_reopenings.entry(name.to_string()).or_default();
                if files.last() != Some(path) {
                    files.push(path.clone());
                }
            }
            type_test |= is_type_test_def(&line);
        }
        if type_test {
            index.type_test_files.push(path.clone());
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
    fn scans_bare_toplevel_const_writes() {
        assert_eq!(scan_toplevel_const_write("XClient = T.let(T.unsafe(nil), T.nilable(Integer))"), Some("XClient"));
        assert_eq!(scan_toplevel_const_write("DRbIdConv = DRb::DRbIdConv"), Some("DRbIdConv"));
        assert_eq!(scan_toplevel_const_write("  MODULES = T.let(T.unsafe(nil), T::Array[T.untyped])"), Some("MODULES"));
    }

    #[test]
    fn ignores_non_assignment_and_qualified_toplevel_lines() {
        assert_eq!(scan_toplevel_const_write("Foo::Bar = 1"), None, "qualified writes are a separate channel");
        assert_eq!(scan_toplevel_const_write("FOO == 1"), None);
        assert_eq!(scan_toplevel_const_write("FOO =~ /x/"), None);
        assert_eq!(scan_toplevel_const_write("FOO => bar"), None);
        assert_eq!(scan_toplevel_const_write("# FakeConst = 1"), None);
        assert_eq!(scan_toplevel_const_write("class Foo"), None);
        assert_eq!(scan_toplevel_const_write(""), None);
    }

    #[test]
    fn build_rbi_index_unions_bare_toplevel_const_writes() {
        // ponytail: crate target dir, not system temp — same
        // machine-local/wiped-with-clean contract as the other
        // filesystem-backed test below.
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/itaruby-rbi-toplevel-const-test");
        std::fs::create_dir_all(&dir).expect("create test rbi dir");
        std::fs::write(
            dir.join("shim.rbi"),
            "# typed: true\nXClient = T.let(T.unsafe(nil), T.nilable(Integer))\n",
        )
        .expect("write shim.rbi");
        let files = discover_rbi_files(&dir);
        let index = build_rbi_index(&files);
        assert!(
            index.constants.contains_key("XClient"),
            "bare top-level value-constant write must be a phase-1 candidate, got: {:?}",
            index.constants.keys().collect::<Vec<_>>()
        );
        let _ = std::fs::remove_dir_all(&dir);
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
