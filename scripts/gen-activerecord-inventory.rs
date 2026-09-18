// Bead ita-k9j (entrega 2 gate): mechanical harvest of the method names a
// real `ActiveRecord::Base` subclass responds to, from two independent
// sources — never hand-written, same "generated-content exception" the
// anti-gaming rule grants `core_inventory.txt`/`stdlib_constants.txt`.
//
// Source 1 (authoritative, feeds the versioned file): a Tapioca
// `sorbet/rbi/gems/activerecord@*.rbi`'s declared ancestry, walked through
// `itaruby_semantic::index::rbi_method_closure_names` — the SAME BFS
// (`rbi_method_closure`) the checker itself consults at `rbi_escalate`
// time, so this harvest can never disagree with what `ita check` would
// actually resolve. Four edge rules, unchanged from that function's own
// doc comment: `include`/`prepend` keep the current dispatch track;
// `extend`/`mixes_in_class_methods` switch TO the singleton track (this is
// how `Model.where` exists — `where` is an INSTANCE method of
// `ActiveRecord::Querying`, moved to the singleton slot by `ActiveRecord::
// Base`'s own `extend ::ActiveRecord::Querying`); `def self.x`/
// `class << self` always lands on the singleton track regardless of which
// track reached the node. This script invents no second traversal.
//
// The RBI file's `class ActiveRecord::Base` fragment reflects whatever was
// actually mixed into the live class object when Tapioca ran for the ONE
// project supplying it — which can include that project's own Gemfile
// plugins reopening `ActiveRecord::Base` (measured on corpus-c, 2026-08-22:
// `DeepCloneable::DeepClone` and `CounterCulture::Extensions`, two direct
// `include`s neither declared by ActiveRecord nor by ActiveModel/
// ActiveSupport). Those are real public gems, but not ActiveRecord's own
// API and not portable to a corpus with a different Gemfile — this script
// EXCLUDES them by restricting the RBI map the closure walks to files
// whose declared top-level constant starts with `ActiveRecord`,
// `ActiveModel`, or `ActiveSupport` (ActiveRecord::Base's own two
// framework-internal dependencies) before ever calling the closure. An
// edge into an excluded name still gets visited by the BFS; it just never
// resolves to a file, so it contributes zero methods — exactly as if the
// project never declared it. Every excluded DIRECT edge of
// `ActiveRecord::Base` itself is named on stderr, never silently dropped.
//
// Source 2 (cross-check only, reported on stderr, NOT written into the
// file): the unpacked gem source's own literal `include`/`prepend`/
// `superclass` edges, parsed by itaruby_semantic's ordinary project
// indexer (`project_index`, the exact same `DefWalker`/`ancestors`/
// `ClassDef.methods` machinery every `.rb` project file goes through) —
// confirms or refutes AGENTS.md's recorded finding that the RBI declares
// more edges than the source (123 vs 47 on corpus-c) because part of the gap
// (dynamically `const_set`+`include`d generated-attribute modules) is
// unreachable by ANY static parser, source included.
//
// Usage:
//   cargo run --release -p itaruby_semantic --bin gen-activerecord-inventory -- \
//     <path/to/sorbet/rbi/gems> <path/to/activerecord-X.Y.Z/lib> \
//     > crates/itaruby_semantic/declarations/activerecord_api.txt
//
// stdout is EXACTLY the versioned file content (nothing else); the
// harvest report (set sizes, overlap, excluded names) prints to stderr.
//
// Only `sorbet/rbi/gems/` is ever read — never `sorbet/rbi/dsl/`, which is
// a project's own app models, not gem declarations (bead ita-k9j's
// constraint, same secrecy-wall discipline as `declarations/gems.rbi`).
//
// Generated with itaruby_semantic's own `rbi_method_closure` as of this
// script's commit; ActiveRecord's version is whatever the pointed-at RBI
// names (7.2.2.1 at generation time, corpus-c, m5, 2026-08-22).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

fn is_rails_ns(name: &str) -> bool {
    name.starts_with("ActiveRecord") || name.starts_with("ActiveModel") || name.starts_with("ActiveSupport")
}

/// Direct `include`/`extend`/`mixes_in_class_methods` targets textually
/// inside `class_name`'s own top-level block in `file` — diagnostic only
/// (which excluded names to name on stderr), never consulted by the real
/// closure walk above. Tapioca's `gems/` output is column-0 headers with a
/// 2-space-indented body and a column-0 `end`, confirmed by inspection of
/// the real file this bead measured against.
fn direct_edges(file: &Path, class_name: &str) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(file) else {
        return Vec::new();
    };
    let header = format!("class {class_name}");
    let mut in_block = false;
    let mut out = Vec::new();
    for line in text.lines() {
        if !in_block {
            if line == header || line.starts_with(&format!("{header} <")) {
                in_block = true;
            }
            continue;
        }
        if line == "end" {
            break;
        }
        let trimmed = line.trim_start();
        for prefix in ["include ", "extend ", "mixes_in_class_methods "] {
            if let Some(rest) = trimmed.strip_prefix(prefix) {
                out.push(rest.trim_start_matches("::").trim().to_string());
            }
        }
    }
    out
}

fn discover_rb(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            discover_rb(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rb") {
            out.push(path);
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let Some(rbi_gems_dir) = args.get(1) else {
        eprintln!(
            "usage: gen-activerecord-inventory <sorbet/rbi/gems dir> [gem source lib dir]"
        );
        std::process::exit(1);
    };
    let start = "ActiveRecord::Base";

    let all_files = itaruby_semantic::rbi::discover_rbi_files(Path::new(rbi_gems_dir));
    let mut raw_map = itaruby_semantic::rbi::build_rbi_index(&all_files).constants;

    // `build_rbi_index` is first-occurrence-wins over `read_dir`'s
    // unordered walk (rbi.rs's own doc comment) — a real hazard here, not
    // theoretical: on this machine `flipper@1.3.1.rbi` and
    // `activestorage@7.2.2.1.rbi` BOTH also declare a tiny
    // `class ActiveRecord::Base` reopening (3 `include`s each), and one of
    // those can win the race over the real `activerecord@*.rbi`'s 47+
    // edges depending on directory order. Force the three canonical
    // framework files' OWN declared names to win regardless of scan
    // order, by rescanning just those files last and overwriting.
    let canonical_files: Vec<PathBuf> = all_files
        .iter()
        .filter(|p| {
            let name = p.file_name().and_then(|f| f.to_str()).unwrap_or("");
            name.starts_with("activerecord@")
                || name.starts_with("activemodel@")
                || name.starts_with("activesupport@")
        })
        .cloned()
        .collect();
    eprintln!("canonical gem files pinned: {canonical_files:?}");
    for (name, files) in itaruby_semantic::rbi::build_rbi_index(&canonical_files).constants {
        raw_map.insert(name, files);
    }

    let base_file = raw_map.get(start).and_then(|files| files.first().cloned());
    let excluded: Vec<String> = base_file
        .as_deref()
        .map(|f| {
            direct_edges(f, start)
                .into_iter()
                .filter(|n| !is_rails_ns(n))
                .collect()
        })
        .unwrap_or_default();

    let rails_map: std::collections::HashMap<String, Vec<PathBuf>> = raw_map
        .into_iter()
        .filter(|(k, _)| is_rails_ns(k))
        .collect();

    let (instance, singleton) =
        itaruby_semantic::index::rbi_method_closure_names(start, &rails_map);
    let instance_set: HashSet<&str> = instance.iter().map(String::as_str).collect();
    let singleton_set: HashSet<&str> = singleton.iter().map(String::as_str).collect();

    // Source 2: gem source cross-check, instance track only (superclass /
    // include / prepend — matches AGENTS.md's prior `ancestors`/
    // `lookup_method` measurement; `extend` is deliberately not walked a
    // second time here, same scope that measurement used).
    let mut source_set: HashSet<String> = HashSet::new();
    if let Some(src_dir) = args.get(2) {
        let mut rb_files = Vec::new();
        discover_rb(Path::new(src_dir), &mut rb_files);
        let db = itaruby_semantic::Db::default();
        let sources: Vec<_> = rb_files
            .iter()
            .map(|p| {
                let text = std::fs::read_to_string(p).unwrap_or_default();
                itaruby_semantic::SourceFile::new(&db, p.clone(), text)
            })
            .collect();
        let file_count = sources.len();
        itaruby_semantic::ProjectFiles::new(&db, sources);
        let idx = itaruby_semantic::project_index(&db);
        if let Some(&id) = idx.by_path.get(start) {
            let (ancestors, complete) = idx.ancestors(id);
            for a in &ancestors {
                source_set.extend(idx.class(*a).methods.keys().cloned());
            }
            eprintln!(
                "source: {file_count} .rb files, {} ancestor nodes, ancestry complete = {complete}",
                ancestors.len()
            );
        } else {
            eprintln!("source: {start} not found under {src_dir} — skipping cross-check");
        }
    }

    let overlap = instance_set
        .iter()
        .filter(|m| source_set.contains(**m))
        .count();

    eprintln!("rbi instance methods:   {}", instance_set.len());
    eprintln!("rbi singleton methods:  {}", singleton_set.len());
    eprintln!("source instance methods (cross-check): {}", source_set.len());
    eprintln!("overlap (rbi instance ∩ source):        {overlap}");
    eprintln!(
        "excluded direct edges of {start} (not ActiveRecord/ActiveModel/ActiveSupport): {excluded:?}"
    );

    println!("# itaruby ActiveRecord API inventory (bead ita-k9j) — GENERATED FILE, do not hand-edit.");
    println!("#");
    println!("# One `ActiveRecord::Base#method` (instance) or");
    println!("# `ActiveRecord::Base.method` (singleton) line per public method reachable");
    println!("# from `ActiveRecord::Base` through its Tapioca-declared ancestry.");
    println!("#");
    println!("# SOURCE 1 (this file): sorbet/rbi/gems/activerecord@*.rbi's declared");
    println!("# ancestry, walked with itaruby_semantic::index::rbi_method_closure_names —");
    println!("# the exact BFS (rbi_method_closure) `ita check` itself consults at");
    println!("# rbi_escalate time. Restricted to ActiveRecord::/ActiveModel::/");
    println!("# ActiveSupport:: namespaces only: {} direct edge(s) of ActiveRecord::Base", excluded.len());
    println!("# were excluded as not-ActiveRecord's-own-API — {excluded:?}.");
    println!(
        "# Measured this run: {} instance names below, {} singleton names below.",
        instance_set.len(),
        singleton_set.len()
    );
    println!("#");
    println!("# SOURCE 2 (cross-check only, NOT embedded here): the unpacked");
    println!("# activerecord gem source's own include/prepend/superclass edges via");
    println!("# itaruby_semantic's ordinary project indexer. Sizes and overlap are");
    println!("# reported on stderr by this generator, never folded into this file — the");
    println!(
        "# RBI is the larger, authoritative set (see AGENTS.md, 2026-08-22): this run",
    );
    println!(
        "# measured {} source instance methods, {overlap} inside the RBI instance set.",
        source_set.len()
    );
    println!("#");
    println!("# Mechanically harvested — never written by hand; the generator is");
    println!("# versioned at scripts/gen-activerecord-inventory.rs (the anti-gaming");
    println!("# rule's generated-content exception, same pattern as core_inventory.txt).");
    println!("# Regenerate with:");
    println!("#");
    println!("#   cargo run --release -p itaruby_semantic --bin gen-activerecord-inventory -- \\");
    println!("#     <path/to/sorbet/rbi/gems> <path/to/activerecord-X.Y.Z/lib> \\");
    println!("#     > crates/itaruby_semantic/declarations/activerecord_api.txt");
    println!("#");
    println!("# Generated against ActiveRecord 7.2.2.1's Tapioca RBI (corpus-c, m5, 2026-08-22).");
    for m in &instance {
        println!("{start}#{m}");
    }
    for m in &singleton {
        println!("{start}.{m}");
    }
}
