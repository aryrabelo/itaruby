//! Discovery of a Ruby project's DECLARATION sources — `db/schema.rb`,
//! `db/structure.sql`, and a client's Tapioca-generated `sorbet/rbi/` tree —
//! searched upward from one or more checked roots and wired into the salsa
//! inputs `check_file`/`project_index` read. Shared by `ita check`
//! (`crates/itaruby/src/main.rs`) and `ita server`
//! (`crates/itaruby_server`): both need the exact same `schema.rb` >
//! `structure.sql` precedence and the exact same `sorbet/rbi` phase-1 scan,
//! so the search + wiring lives here once instead of twice (bead ita-16g).
//!
//! Also here (bead ita-547): a `Gemfile.lock` found the same way, mapped
//! gem-name-by-gem-name to a guessed Ruby namespace and wired through
//! `GemfileLockNamespaces` — generalizes mechanism B (bead ita-h6l,
//! `index.rs`'s `is_known_external_class_path`) past its curated
//! core/stdlib/`gems.rbi` lists. See `gem_namespace`'s doc comment for the
//! mapping heuristic and `index.rs`'s `apply_gem_reopenings` for the
//! consumer.
//!
//! Deliberately NOT here: `ClosedWorld` (bead ita-2ve/w12) — that stays
//! wired only by the CLI's `wire_closed_world`/`tapioca_coverage_complete`
//! (`crates/itaruby/src/main.rs`), never by the server, so `ita server`/LSP
//! keep v0's exact silent behavior. See `ClosedWorld`'s doc comment in
//! `lib.rs`.

use std::path::{Path, PathBuf};

use crate::{Db, RbiCoreReopenings, RbiProject, SourceFile, StructureSqlProject};

/// What `wire_declaration_sources` found and wired.
///
/// `schema_rb` names the first `db/schema.rb` found (for logging);
/// `declarations_only` is EVERY `db/schema.rb` path found across `roots` —
/// the caller must add each one to its own project sources (`ProjectFiles`)
/// as a DECLARATION, never as code under review: excluded from
/// `check_file`/diagnostics/`--stats`, exactly like `crates/itaruby/src/
/// main.rs`'s old `declarations_only` list.
///
/// `structure_sql`/`rbi_dir` are reported for logging only — both are
/// already wired into their own salsa singletons (`StructureSqlProject`,
/// `RbiProject`/`RbiCoreReopenings`) by this function; the caller never
/// touches them again.
#[derive(Debug, Default, Clone)]
pub struct DiscoveredSources {
    pub schema_rb: Option<PathBuf>,
    pub structure_sql: Option<PathBuf>,
    pub rbi_dir: Option<PathBuf>,
    pub declarations_only: Vec<PathBuf>,
}

/// Searches upward from every root in `roots` for `db/schema.rb`,
/// `db/structure.sql`, and `sorbet/rbi`, then wires whatever it finds into
/// `db`'s salsa inputs. `schema.rb` always wins over `structure.sql` (bead
/// ita-muf contract): every root is checked for `schema.rb` first, and
/// `structure.sql` is looked up at all only if none of the roots found
/// one — the two are never merged, never mixed per-root. Absent everything
/// is silence: zero salsa inputs wired, an empty `DiscoveredSources`.
pub fn wire_declaration_sources(db: &mut Db, roots: &[PathBuf]) -> DiscoveredSources {
    let mut declarations_only = Vec::new();
    for root in roots {
        if let Some(schema) = find_upward(root, "schema.rb") {
            if !declarations_only.contains(&schema) {
                declarations_only.push(schema);
            }
        }
    }

    let structure_sql = if declarations_only.is_empty() {
        let sql = roots.iter().find_map(|r| find_upward(r, "structure.sql"));
        if let Some(sql_path) = &sql {
            wire_structure_sql(db, sql_path);
        }
        sql
    } else {
        None
    };

    let rbi_dir = roots.iter().find_map(|r| find_upward_dir(r, "sorbet/rbi"));
    if let Some(dir) = &rbi_dir {
        wire_rbi(db, dir);
    }

    if let Some(lock) = roots.iter().find_map(|r| find_upward_bare_file(r, "Gemfile.lock")) {
        wire_gemfile_lock_namespaces(db, &lock);
    }

    DiscoveredSources {
        schema_rb: declarations_only.first().cloned(),
        structure_sql,
        rbi_dir,
        declarations_only,
    }
}

/// Reads `db/structure.sql` and wires it through `StructureSqlProject`,
/// never through `ProjectFiles` — it isn't Ruby, and nothing in this crate
/// may ever run `file_defs`/`check_file`/prism over it.
fn wire_structure_sql(db: &Db, sql_path: &Path) {
    match std::fs::read_to_string(sql_path) {
        Ok(text) => {
            let sql_file = SourceFile::new(db, sql_path.to_path_buf(), text);
            StructureSqlProject::new(db, sql_file);
        }
        Err(err) => eprintln!("itaruby: cannot read {}: {err}", sql_path.display()),
    }
}

/// Phase 1 scan (bead ita-vto, extended to a union by ita-k9j.3) of a
/// client's Tapioca-generated `sorbet/rbi/`: `constant name -> EVERY
/// declaring file` plus every core namespace reopening (w12 closure),
/// each wired as its own salsa singleton, absent-if-empty.
fn wire_rbi(db: &Db, rbi_dir: &Path) {
    let files = crate::rbi::discover_rbi_files(rbi_dir);
    let index = crate::rbi::build_rbi_index(&files);
    if !index.core_reopenings.is_empty() {
        RbiCoreReopenings::new(db, index.core_reopenings);
    }
    if !index.constants.is_empty() {
        RbiProject::new(db, index.constants);
    }
}

/// Singleton input: gem name -> guessed Ruby namespace, for every gem
/// this project's `Gemfile.lock` declares (bead ita-547). See
/// `gem_namespace`'s doc comment for the mapping heuristic and its
/// fail-closed contract, and `index.rs`'s `apply_gem_reopenings` for the
/// one place this feeds a diagnosis decision: a class the project itself
/// reopens whose top-level path segment matches one of these guessed
/// namespaces exactly is forced open (`OpenReason::ReopenedExternal`),
/// generalizing mechanism B (bead ita-h6l) past its curated
/// core/stdlib/`gems.rbi` lists. Wired here, unconditionally, for BOTH
/// `ita check` and `ita server`/LSP — unlike `ClosedWorld`, mechanism B
/// itself is never gated behind closed-world (see `index.rs::walk_stmt`'s
/// `is_known_external_class_path` call), so this generalization keeps
/// that same unconditional behavior. Absent (no lock found, or a lock
/// naming zero mappable gems) is zero cost: every consumer degrades to
/// today's exact silence.
#[salsa::input(singleton)]
pub struct GemfileLockNamespaces {
    #[returns(ref)]
    pub namespaces: std::collections::HashSet<String>,
}

/// Reads a discovered `Gemfile.lock`, maps every gem name it declares to
/// a guessed Ruby namespace (`gem_namespace`), and wires the resulting set
/// through `GemfileLockNamespaces` — read once here, never per class (see
/// that struct's doc comment for why `index.rs`'s `apply_gem_reopenings`
/// must never re-parse this file per fragment). Absent/empty is silence:
/// zero salsa input wired, same contract as `wire_rbi`/`wire_structure_sql`.
fn wire_gemfile_lock_namespaces(db: &Db, lock_path: &Path) {
    let Ok(text) = std::fs::read_to_string(lock_path) else {
        return;
    };
    let namespaces: std::collections::HashSet<String> = parse_gemfile_lock_gem_names(&text)
        .iter()
        .filter_map(|gem| gem_namespace(gem))
        .map(|ns| gem_namespace_key(&ns))
        .collect();
    if !namespaces.is_empty() {
        GemfileLockNamespaces::new(db, namespaces);
    }
}

/// Distinct gem names from a `Gemfile.lock`'s `GEM`/`specs:` blocks — the
/// 4-space-indented `name (version)` lines; 6-space lines under each are
/// dependency constraints, not declarations. Same grammar as `crates/
/// itaruby/src/main.rs`'s `parse_gemfile_lock_gems` (that copy feeds
/// Tapioca coverage completeness, a CLI-only concern per this module's own
/// "deliberately not here" contract for `ClosedWorld`); duplicated rather
/// than shared because the two live in different crates and this copy
/// must run for `ita server`/LSP too.
fn parse_gemfile_lock_gem_names(text: &str) -> Vec<String> {
    let mut gems: Vec<String> = Vec::new();
    let mut in_gem_specs = false;
    for line in text.lines() {
        if !line.starts_with(' ') && !line.is_empty() {
            in_gem_specs = line == "GEM";
            continue;
        }
        if !in_gem_specs {
            continue;
        }
        let Some(rest) = line.strip_prefix("    ") else {
            continue;
        };
        if rest.starts_with(' ') {
            continue;
        }
        let Some(open) = rest.find(" (") else {
            continue;
        };
        let name = &rest[..open];
        if !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            && !gems.iter().any(|g| g == name)
        {
            gems.push(name.to_string());
        }
    }
    gems
}

/// Bead ita-547: generalizes mechanism B (bead ita-h6l, `index.rs`'s
/// `is_known_external_class_path`) beyond its small curated core/stdlib/
/// `gems.rbi` lists. A gem this project's own `Gemfile.lock` names but
/// mechanism B's curated set never covers (measured, Round-5 audit:
/// `mail`, `wikicloth`, `fastimage` — an initializer reopening
/// `Mail::SMTP`/`WikiCloth`/`FastImage` to override one or two methods
/// turns every OTHER project-invisible method on that class into a false
/// E0101, because the reopening looks like a complete project class
/// definition) still needs its ancestry treated as unknown, for the exact
/// same reason mechanism B does: this checker never modeled the real
/// gem's method surface.
///
/// Mapping a `Gemfile.lock` name to its Ruby namespace is not mechanical
/// (`mail` -> `Mail`, `wikicloth` -> `WikiCloth`, `fastimage` ->
/// `FastImage`): a plain "split on `_`/`-`, capitalize each segment"
/// camelize gets the common case (`will_paginate` -> `WillPaginate`) but
/// not a single word a gem author capitalized internally by hand. A small
/// override table covers the measured exceptions; anything else falls
/// through to plain camelize.
///
/// FAIL-CLOSED BY CONSTRUCTION, not by a curated allowlist of gems:
/// `index.rs`'s `apply_gem_reopenings` only forces a class open when its
/// path's TOP-LEVEL segment matches this GUESSED namespace by exact
/// string equality. A wrong guess (the plain camelize of `activesupport`
/// produces `Activesupport`, not the real `ActiveSupport`) can only ever
/// cause a MISS — a false negative, invariant #1's accepted cost — never
/// a false open of some unrelated project namespace: that would require
/// the project to already define a class under the exact guessed
/// spelling, and even then the only effect is "ancestry now unknown", not
/// a fabricated diagnostic. An unrecognized shape (empty, non-identifier
/// characters) returns `None` rather than guess: absent from the mapped
/// set is the same safe default as "gem not in the lock at all".
fn gem_namespace(gem: &str) -> Option<String> {
    // Measured exceptions, and the ONLY kind that can still exist now
    // that matching is case- and separator-insensitive
    // (`gem_namespace_key`): a gem whose namespace differs in LETTERS,
    // not merely in capitalization. The old `wikicloth`/`fastimage`
    // entries were the capitalization kind and are gone with that
    // change — `WikiCloth` and `FastImage` now key to the same
    // `wikicloth`/`fastimage` their lock names do.
    //
    // Each entry below is read out of the gem's own source at the
    // version a reference corpus locks, never inferred from a corpus
    // reopening (that inference is what `gem_namespace_key`'s doc
    // comment refuses, with the measurement):
    //
    // * `kt-paperclip` 8.0.0 (mastodon `Gemfile.lock:394`) defines
    //   `module Paperclip` at `lib/paperclip.rb:82`.
    // * `ruby-vips` 2.3.0 (mastodon `Gemfile.lock:802`) defines
    //   `module Vips` at `lib/vips/image.rb:9` (and in every other
    //   `lib/vips/*.rb`).
    // * `queue_classic` 4.0.0 (rails `Gemfile.lock:423`) defines
    //   `module QC` at `lib/queue_classic.rb:5` (read out of the gem
    //   archive at that exact version, not inferred from the corpus
    //   reopening). The lock name and the constant share no letters at
    //   all, so `gem_namespace_key` cannot pair them: this is precisely
    //   the "differs in LETTERS, not merely in capitalization" case this
    //   table exists for. Measured as 2 of rails' 33 class-object
    //   residue records — `QC.default_conn_adapter` and
    //   `QC.default_conn_adapter=` at
    //   `activejob/test/support/integration/adapters/queue_classic.rb:35-36`,
    //   against a project fragment (`activejob/test/support/
    //   queue_classic/inline.rb:4`) that reopens `QC` only to redefine
    //   three `QC::Queue` methods.
    match gem {
        "kt-paperclip" => return Some("Paperclip".to_string()),
        "ruby-vips" => return Some("Vips".to_string()),
        "queue_classic" => return Some("QC".to_string()),
        _ => {}
    }
    if gem.is_empty() {
        return None;
    }
    let mut out = String::with_capacity(gem.len());
    for part in gem.split(['_', '-']) {
        let mut chars = part.chars();
        match chars.next() {
            Some(c) if c.is_ascii_alphabetic() => {
                out.push(c.to_ascii_uppercase());
                for c in chars {
                    if !c.is_ascii_alphanumeric() {
                        return None;
                    }
                    out.push(c);
                }
            }
            _ => return None,
        }
    }
    Some(out)
}

/// The key both sides of `apply_gem_reopenings`' comparison are reduced
/// to: ASCII-lowercase, separators dropped. A `Gemfile.lock` name and the
/// constant it names agree on LETTERS and never on case or punctuation —
/// `rspec` is `RSpec`, `connection_pool` is `ConnectionPool`,
/// `message_bus` is `MessageBus`, `activesupport` is `ActiveSupport` —
/// so comparing the camelize GUESS by exact string equality (what this
/// mechanism did until 2026-09-17) missed every gem whose author placed a
/// capital anywhere the segment boundaries do not predict. Measured on
/// the reference corpora by reopening site: mastodon reopens
/// `ConnectionPool::*` (2 sites) and discourse `MessageBus::*` (1 site)
/// under namespaces their own locks declare, and both were invisible
/// here.
///
/// Case-insensitivity cannot widen this mechanism past the gems a lock
/// actually names: the key is still the WHOLE name, so it can only ever
/// pair a lock entry with the constant spelling of that same entry.
/// SEGMENT matching would be the unsafe generalization, and it is
/// deliberately refused — measured on the same corpora, "some hyphen
/// segment of some locked gem equals this namespace" would have blinded
/// `Api` (145 reopening sites in mastodon, via `elasticsearch-api`),
/// `Auth` (16 in discourse, via `auth-sanitizer`), `Scheduler`, `Form`,
/// `Event` and `Web`: all of them the PROJECT's own namespaces, and every
/// method on them would have stopped being checkable.
pub fn gem_namespace_key(namespace: &str) -> String {
    let mut out = String::with_capacity(namespace.len());
    for c in namespace.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        }
    }
    out
}

/// Same walk as `find_upward`/`find_upward_dir`, for a bare file name
/// sitting directly in a candidate directory rather than under `db/` —
/// `Gemfile.lock` (bead ita-547). Not shared with `crates/itaruby/src/
/// main.rs`'s own private `find_upward_file` (CLI-only, feeds
/// `tapioca_coverage_complete`): same reason `parse_gemfile_lock_gem_names`
/// above is its own copy rather than reusing that one.
fn find_upward_bare_file(root: &Path, name: &str) -> Option<PathBuf> {
    upward_dirs(root)
        .into_iter()
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

/// The candidate directories of an upward walk, nearest first, at most 4
/// levels up — and the one place that resolves the root.
///
/// A root reached through a symlink has LEXICAL parents that are not its
/// real ones: checking a symlinked `app/` walks up through the *link's*
/// directory, so a `Gemfile` sitting beside the real `app/` is invisible.
/// That absence is not neutral — the closed-world gate reads "no Gemfile"
/// as "this project has no gems", which is the license to call every
/// `ActiveSupport` core extension an undefined method (2026-08-24: 310
/// fabricated errors on a corpus whose real answer is 2).
///
/// So each level yields the caller's own form FIRST and the resolved form
/// only as a fallback. That order is load-bearing in both directions: the
/// resolved form is what makes a symlinked root find its project, and the
/// caller's form is what keeps the returned path comparable to the paths
/// the caller already holds — `declarations_only` is membership-tested
/// against LSP document paths and against `discover_rb_files` output, so
/// handing back a canonicalized path where nothing else is canonicalized
/// silently stops matching (that regression cost one gate run to find).
///
/// An empty result means "could not even resolve the root" — "could not
/// tell", never "not there", the same distinction as `ita check`'s
/// `try_exists`.
fn upward_dirs(root: &Path) -> Vec<PathBuf> {
    let lexical = match (root.is_dir(), root.parent()) {
        (true, _) => Some(root.to_path_buf()),
        (false, Some(p)) => Some(p.to_path_buf()),
        (false, None) => None,
    };
    let resolved = upward_start(root);
    let mut out = Vec::with_capacity(8);
    let mut lex = lexical;
    let mut res = resolved;
    for _ in 0..4 {
        for cand in [lex.clone(), res.clone()].into_iter().flatten() {
            if !out.contains(&cand) {
                out.push(cand);
            }
        }
        lex = lex.and_then(|d| d.parent().map(Path::to_path_buf));
        res = res.and_then(|d| d.parent().map(Path::to_path_buf));
    }
    out
}

/// The resolved starting directory of an upward walk, or `None` when the
/// root cannot be resolved at all. Public because `gems_detected` in the
/// CLI needs to distinguish "resolved" from "could not tell" to stay
/// fail-closed: an unresolvable root reports gems PRESENT.
pub fn upward_start(root: &Path) -> Option<PathBuf> {
    let real = std::fs::canonicalize(root).ok()?;
    if real.is_dir() {
        Some(real)
    } else {
        Some(real.parent()?.to_path_buf())
    }
}

/// Walk upward from `root` (its parent, if `root` is a file) up to 4
/// directory levels looking for `db/<name>`. First hit wins; no hit is
/// silence.
fn find_upward(root: &Path, name: &str) -> Option<PathBuf> {
    upward_dirs(root)
        .into_iter()
        .map(|dir| dir.join("db").join(name))
        .find(|candidate| candidate.is_file())
}

/// Same walk as `find_upward`, generalized to a directory instead of a
/// single `db/<name>` file — bead ita-vto's `sorbet/rbi` search. `rel` is a
/// path relative to each candidate directory (e.g. `"sorbet/rbi"`). Public:
/// also reused by `crates/itaruby/src/main.rs`'s `tapioca_coverage_complete`
/// (w12 closure) for `sorbet/rbi/gems`, the one shared piece between the
/// declaration-source search and the CLI-only closed-world check.
pub fn find_upward_dir(root: &Path, rel: &str) -> Option<PathBuf> {
    upward_dirs(root)
        .into_iter()
        .map(|dir| dir.join(rel))
        .find(|candidate| candidate.is_dir())
}
