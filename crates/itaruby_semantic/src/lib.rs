//! Semantic layer: definition index, type inference, diagnostics.

pub mod check;
pub mod core;
pub mod declarations;
pub mod discovery;
pub mod index;
pub mod rbi;
pub mod rbs_comment;
pub mod schema;
pub mod sorbet_sig;
pub mod structure_sql;
pub mod types;

pub use check::{
    call_stats, check_file, check_file_dark, constraint_report, definition_at, hover_at, ty_name,
    CallStats, ConstraintOutcome, DarkSingleton, DarkVerdict, DefSite, HoverInfo, MethodHover,
};
pub use discovery::{find_upward_dir, upward_start, wire_declaration_sources, DiscoveredSources};
pub use index::{core_methods_of, file_defs, project_consts, project_index, rbi_core_methods, rbi_files_parsed_count, stdlib_singleton_method, Blocker, FileDefs, OpenReason, ProjectIndex};
pub use itaruby_syntax::{LineIndex, SourceFile};
pub use types::{
    ConstraintCall, ConstraintProof, Diagnostic, Severity, Ty, E0107_CONSTRAINT_CONTRADICTION,
    E0108_OPERAND_TYPE_MISMATCH, E0109_RETURN_TYPE_MISMATCH,
};

/// The concrete database. All queries take `&dyn salsa::Database`.
pub type Db = salsa::DatabaseImpl;

/// Singleton input: every `.rb` file in the workspace. `project_index`
/// derives from this; the server updates it on file create/delete.
#[salsa::input(singleton)]
pub struct ProjectFiles {
    #[returns(ref)]
    pub files: Vec<SourceFile>,
}

/// Singleton input: `db/structure.sql`'s `SourceFile`, when a project has
/// no `db/schema.rb` (bead ita-muf; corpus-b is the reference corpus — a
/// real Postgres dump, no `schema.rb` at all). Deliberately separate from
/// `ProjectFiles`: this `SourceFile` is never pushed into that vec, so
/// nothing in this crate can ever run `file_defs`/`check_file`/prism over
/// it — declarations-only by construction, not by a diagnostic-time
/// filter (`db/schema.rb`'s `declarations_only` list in
/// `crates/itaruby/src/main.rs` doesn't apply here and doesn't need to).
/// See `structure_sql.rs` for the parser and `index.rs::project_index` for
/// how this and a `db/schema.rb` found inside `ProjectFiles` are mutually
/// exclusive — `schema.rb` always wins when both are wired.
#[salsa::input(singleton)]
pub struct StructureSqlProject {
    pub file: SourceFile,
}

/// Singleton input: `constant name -> EVERY declaring .rbi file` (bead
/// ita-vto, extended to a union by bead ita-k9j.3), the phase-1 map
/// `rbi::build_rbi_index` produces by scanning a client project's own
/// `sorbet/rbi/**/*.rbi` (Tapioca output) — never set when no such
/// directory is discovered, matching every other discoverable-
/// declaration-source contract in this crate (absent input, zero cost).
/// `Vec`, not a single `PathBuf`: a name redeclared across several
/// files — a stub reopening plus the real declaration, or several gems
/// each reopening the same core class — is a real reopening at runtime,
/// so every consumer must union all of them, never pick one arbitrarily
/// (see `rbi::RbiIndex`'s doc comment for the measured hazard this
/// fixes). Consumed lazily, one constant at a time, by
/// `check.rs::Checker::check_const_ref` via `index::rbi_declares`; see
/// that function's doc comment for the full contract.
#[salsa::input(singleton)]
pub struct RbiProject {
    #[returns(ref)]
    pub map: std::collections::HashMap<String, Vec<std::path::PathBuf>>,
}

/// Singleton input: core namespace name -> EVERY `.rbi` file that reopens
/// it (w12 closure, Tapioca closed world). The second map `rbi::build_rbi_index`
/// harvests in the same phase-1 line scan as `RbiProject`'s constants —
/// no extra pass. Absent (no client `sorbet/rbi`) is zero cost; the parse
/// it feeds (`index::rbi_core_methods`) only ever runs inside
/// closed-world's conclusive core lookup, so open-world projects never
/// pay for it.
#[salsa::input(singleton)]
pub struct RbiCoreReopenings {
    #[returns(ref)]
    pub map: std::collections::HashMap<String, Vec<std::path::PathBuf>>,
}

/// Singleton input: closed-world mode (bead ita-2ve). Wired to `true`
/// ONLY by `ita check`, after it proved every checked root is safe one of
/// two ways: no `Gemfile`/`Gemfile.lock`/`*.gemspec` exists upward of it,
/// OR a `Gemfile.lock` exists and Tapioca coverage is complete — every
/// gem in its GEM/specs sections has a `sorbet/rbi/gems/<name>@*.rbi`, so
/// the monkeypatches gems make to core classes are exactly what those
/// RBIs declare (w12 closure; discovery lives in the binary,
/// `crates/itaruby/src/main.rs`, same layering as `StructureSqlProject`).
/// Absent — `ita server`, LSP, library tests that don't opt in — means
/// OFF, and every behavior it gates (core-class narrowing, the conclusive
/// core lookup) is byte-identical to v0: silence (invariant #1). With it
/// on, an unknown method on a concrete core receiver can conclude E0101
/// only when the project also never reopened the class and no gem RBI
/// declares the method (see `check.rs`'s `core_unknown_is_conclusive`).
#[salsa::input(singleton)]
pub struct ClosedWorld {
    pub enabled: bool,
}
