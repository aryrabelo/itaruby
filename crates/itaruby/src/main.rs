use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use itaruby_semantic::{
    call_stats, check_file, constraint_report, definition_at, find_upward_dir, hover_at,
    wire_declaration_sources, CallStats, ConstraintCall, ConstraintOutcome, ConstraintProof, Db,
    Diagnostic, DiscoveredSources, LineIndex, ProjectFiles, Severity, SourceFile,
};

// bead ita-ufi hotspot 1: the platform allocator (Apple's `libsystem_malloc`)
// was ~24%+ of sampled CPU on the reference corpora — this binary allocates
// heavily (one `String` per checked file, plus the index/checker's own
// working sets) and mimalloc's thread-local free lists beat the general-
// purpose system allocator on that pattern. Scoped to the `ita` BINARY only
// (this crate's `Cargo.toml`, not `itaruby_semantic`/`itaruby_syntax`): a
// `#[global_allocator]` is process-wide, so it can only ever be set once,
// and setting it in a library would force every consumer of that library
// (e.g. `itaruby_server`'s LSP process) into the same choice without a say.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.split_first() {
        Some((cmd, _)) if cmd == "server" => run_server(),
        Some((cmd, rest)) if cmd == "check" && !rest.is_empty() => run_check(rest),
        Some((cmd, rest)) if cmd == "definition" && rest.len() == 1 => run_definition(&rest[0]),
        Some((cmd, rest)) if cmd == "hover" && rest.len() == 1 => run_hover(&rest[0]),
        _ => {
            eprintln!("usage: ita server");
            eprintln!("       ita check [--verbose] [--stats] [--format=agent|json] <path>...");
            eprintln!("       ita definition <path>:<line>:<col>");
            eprintln!("       ita hover <path>:<line>:<col>");
            eprintln!("  --verbose  phase logs (rbi scan/load) go to stderr; without it,");
            eprintln!("             stderr and stdout carry diagnostics only — no phase noise");
            eprintln!("  --stats    call-site coverage census: how much is actually checked");
            eprintln!("  --format=agent  print only constraint-diagnostic markdown blocks,");
            eprintln!("                  one per E0107 contradiction and inferred/union");
            eprintln!("                  declaration candidate, for an AI agent to resolve");
            eprintln!("  --format=json   print one JSON object per diagnostic (JSONL) instead");
            eprintln!("                  of the human-readable excerpt report");
            ExitCode::from(2)
        }
    }
}

fn run_server() -> ExitCode {
    match itaruby_server::run_server() {
        Ok(()) => ExitCode::from(0),
        Err(err) => {
            eprintln!("itaruby server: {err:?}");
            ExitCode::from(1)
        }
    }
}

/// `<path>:<line>:<col>`, 1-based, matching `ita check`'s own diagnostic
/// output. Tolerant of `:` inside `path`: only the last two colon-separated
/// segments are required to be numeric, so `C:\foo.rb:3:1` or a path that
/// legitimately contains a colon still parses.
fn parse_location_arg(arg: &str) -> Option<(&str, u32, u32)> {
    let (rest, col_str) = arg.rsplit_once(':')?;
    let col: u32 = col_str.parse().ok()?;
    let (path, line_str) = rest.rsplit_once(':')?;
    let line: u32 = line_str.parse().ok()?;
    if path.is_empty() || line == 0 || col == 0 {
        return None;
    }
    Some((path, line, col))
}

fn run_definition(arg: &str) -> ExitCode {
    let Some((path_str, line, col)) = parse_location_arg(arg) else {
        eprintln!(
            "itaruby definition: invalid location `{arg}`, expected <path>:<line>:<col> (1-based)"
        );
        return ExitCode::from(2);
    };
    let path = PathBuf::from(path_str);
    let abs_path = match std::fs::canonicalize(&path) {
        Ok(p) => p,
        Err(err) => {
            eprintln!("itaruby definition: cannot read {}: {err}", path.display());
            return ExitCode::from(2);
        }
    };

    // Project scope: every `.rb` file discoverable from the target's
    // directory, same walk `ita check <dir>` uses — cross-file lookups need
    // siblings in the index.
    let root = abs_path
        .parent()
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
    let mut rb_files = Vec::new();
    discover_rb_files(&root, &[], &mut rb_files);
    if !rb_files.contains(&abs_path) {
        rb_files.push(abs_path.clone());
    }

    let db = Db::new();
    let mut sources = Vec::with_capacity(rb_files.len());
    let mut target = None;
    for p in &rb_files {
        let Ok(text) = std::fs::read_to_string(p) else {
            continue;
        };
        let file = SourceFile::new(&db, p.clone(), text);
        if *p == abs_path {
            target = Some(file);
        }
        sources.push(file);
    }
    ProjectFiles::new(&db, sources);

    let Some(target) = target else {
        eprintln!("itaruby definition: cannot read {}", abs_path.display());
        return ExitCode::from(2);
    };

    let text = target.text(&db);
    let offset = LineIndex::new(text).offset(text, line - 1, col - 1);
    match definition_at(&db, target, offset) {
        Some(site) => {
            let site_text = site.file.text(&db);
            let (l, c) = LineIndex::new(site_text).line_col(site_text, site.start);
            println!("{}:{}:{}", site.file.path(&db).display(), l + 1, c + 1);
        }
        None => println!("unknown"),
    }
    ExitCode::from(0)
}

/// `ita hover <path>:<line>:<col>` — the cheap, scriptable twin of the
/// LSP hover (w4): same `hover_at` query, same honesty contract (no info
/// prints `unknown`, never a guess). Project scope mirrors
/// `run_definition`: every `.rb` sibling of the target joins the index,
/// cross-file lookups need them.
fn run_hover(arg: &str) -> ExitCode {
    let Some((path_str, line, col)) = parse_location_arg(arg) else {
        eprintln!(
            "itaruby hover: invalid location `{arg}`, expected <path>:<line>:<col> (1-based)"
        );
        return ExitCode::from(2);
    };
    let path = PathBuf::from(path_str);
    let abs_path = match std::fs::canonicalize(&path) {
        Ok(p) => p,
        Err(err) => {
            eprintln!("itaruby hover: cannot read {}: {err}", path.display());
            return ExitCode::from(2);
        }
    };

    let root = abs_path
        .parent()
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
    let mut rb_files = Vec::new();
    discover_rb_files(&root, &[], &mut rb_files);
    if !rb_files.contains(&abs_path) {
        rb_files.push(abs_path.clone());
    }

    let db = Db::new();
    let mut sources = Vec::with_capacity(rb_files.len());
    let mut target = None;
    for p in &rb_files {
        let Ok(text) = std::fs::read_to_string(p) else {
            continue;
        };
        let file = SourceFile::new(&db, p.clone(), text);
        if *p == abs_path {
            target = Some(file);
        }
        sources.push(file);
    }
    ProjectFiles::new(&db, sources);

    let Some(target) = target else {
        eprintln!("itaruby hover: cannot read {}", abs_path.display());
        return ExitCode::from(2);
    };

    let text = target.text(&db);
    let offset = LineIndex::new(text).offset(text, line - 1, col - 1);
    match hover_at(&db, target, offset) {
        Some(info) => {
            if let Some(ty) = info.ty {
                println!("{ty}");
            }
            if let Some(m) = info.method {
                let site_text = m.site.file.text(&db);
                let (l, _) = LineIndex::new(site_text).line_col(site_text, m.site.start);
                println!(
                    "{} — arity {} — {}:{}",
                    m.name,
                    m.arity,
                    m.site.file.path(&db).display(),
                    l + 1
                );
            }
        }
        None => println!("unknown"),
    }
    ExitCode::from(0)
}

/// bead ita-2ve + w12 closure: closed-world core checking. A root is
/// safe when gems cannot monkeypatch core classes invisibly: either no
/// `Gemfile`/`Gemfile.lock`/`*.gemspec` sits anywhere upward of it
/// (ita-2ve), or the project carries a `Gemfile.lock` with COMPLETE
/// Tapioca coverage (w12 closure) — every gem in the lock's GEM/specs
/// sections has a `sorbet/rbi/gems/<name>@*.rbi`, so the core patches
/// gems make are exactly what those RBIs declare, and the conclusive
/// lookup consults them (`check.rs`'s condition (d)). Every checked root
/// must be safe; one unsafe root keeps the whole run open (silence).
/// Wired only when clean, and only here: `ita server`/LSP never wire it,
/// so they keep v0's exact behavior (see `ClosedWorld`).
fn wire_closed_world(db: &Db, paths: &[String]) {
    if paths.iter().all(|p| {
        let root = Path::new(p);
        !gems_detected(root) || tapioca_coverage_complete(root)
    }) {
        itaruby_semantic::ClosedWorld::new(db, true);
    }
}

/// Does `root`'s project have complete Tapioca coverage — a
/// `Gemfile.lock` upward, a `sorbet/rbi/gems/` upward (same 4-level
/// walk), and an RBI for EVERY gem the lock's GEM/specs sections name?
/// Any miss (lock absent, gems dir absent, one gem without an RBI, an
/// unreadable lock) fails closed: the old silence, exactly as if no
/// Tapioca existed. Platform-suffixed versions (`wal-pro@1.0-arm64`)
/// come along for free — `<name>@*.rbi` matches on the name prefix.
fn tapioca_coverage_complete(root: &Path) -> bool {
    let Some(lock) = find_upward_file(root, "Gemfile.lock") else {
        return false;
    };
    let Some(gems_dir) = find_upward_dir(root, "sorbet/rbi/gems") else {
        return false;
    };
    let Ok(text) = std::fs::read_to_string(&lock) else {
        return false;
    };
    let gems = parse_gemfile_lock_gems(&text);
    if gems.is_empty() {
        // ponytail: a lock with zero GEM/specs entries is gemless; PATH/
        // GIT gems still need their own coverage story before this can
        // open — fail closed until then.
        return false;
    }
    let covered = gem_rbi_names(&gems_dir);
    gems.iter().all(|g| covered.contains(g))
}

/// Distinct gem names from a `Gemfile.lock`'s GEM sections' `specs:`
/// blocks — the 4-space-indented `name (version)` lines. The 6-space
/// lines under each are dependency constraints, not declarations; every
/// gem in the graph gets its own 4-space line. Multiple GEM sections
/// (private sources) all count. Hand-rolled, stdlib-only: the format is
/// two `grep`-able shapes and a section-name guard.
fn parse_gemfile_lock_gems(text: &str) -> Vec<String> {
    let mut gems: Vec<String> = Vec::new();
    let mut in_gem_specs = false;
    for line in text.lines() {
        if !line.starts_with(' ') && !line.is_empty() {
            // A section header at column 0: `GEM`, `PLATFORMS`, ... —
            // `GEM` re-enters, anything else leaves.
            in_gem_specs = line == "GEM";
            continue;
        }
        if !in_gem_specs {
            continue;
        }
        // Exactly 4 spaces of indent = a declared spec; more = a
        // dependency constraint under it.
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

/// Gem names with an RBI on disk: `<name>@<version>[.platform].rbi`
/// stems cut at the first `@` (gem names never contain one).
fn gem_rbi_names(gems_dir: &Path) -> std::collections::HashSet<String> {
    let mut names = std::collections::HashSet::new();
    let Ok(entries) = std::fs::read_dir(gems_dir) else {
        return names;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("rbi") {
            continue;
        }
        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            if let Some(at) = stem.find('@') {
                names.insert(stem[..at].to_string());
            }
        }
    }
    names
}

/// Hand-parsed `ita check` flags (no clap, same as the rest of the CLI).
/// `None` means an unknown `--format=` value: the caller prints usage and
/// exits 2.
struct CheckFlags {
    verbose: bool,
    stats: bool,
    format_agent: bool,
    format_json: bool,
}

fn parse_check_flags(args: &[String]) -> Option<CheckFlags> {
    let mut format_agent = false;
    let mut format_json = false;
    for a in args {
        match a.strip_prefix("--format=") {
            Some("agent") => format_agent = true,
            Some("json") => format_json = true,
            Some(_) => return None,
            None => {}
        }
    }
    Some(CheckFlags {
        verbose: args.iter().any(|a| a == "--verbose" || a == "-v"),
        stats: args.iter().any(|a| a == "--stats"),
        format_agent,
        format_json,
    })
}

fn run_check(args: &[String]) -> ExitCode {
    let usage = "usage: ita check [--verbose] [--stats] [--format=agent|json] <path>...";
    let Some(flags) = parse_check_flags(args) else {
        eprintln!("{usage}");
        return ExitCode::from(2);
    };
    let paths: Vec<String> = args
        .iter()
        .filter(|a| !a.starts_with('-'))
        .cloned()
        .collect();
    if paths.is_empty() {
        eprintln!("{usage}");
        return ExitCode::from(2);
    }
    // bead ita-76m.2: a root that does not exist is a usage error, not a
    // clean run. `discover_rb_files` walks `read_dir`, so a missing path
    // yields zero files, zero diagnostics and exit 0 — byte-identical to
    // "your code is fine". In CI (a typo'd path, the wrong
    // working-directory, a shallow checkout) that green means nothing was
    // checked at all. An existing directory holding no `.rb` stays silent
    // and exit 0 on purpose: checking a subtree that has no Ruby yet is
    // legitimate, and the silence-when-empty contract is load-bearing.
    //
    // `try_exists`, never `exists()`: the latter collapses "definitely not
    // there" and "could not find out" into the same `false`. A root whose
    // parent directory denies traversal, or a mount that is gone, is not a
    // typo — telling the operator "path not found" would send them looking
    // for the wrong bug. Both still exit 2, because both mean this run
    // checked nothing; only the message differs, and the message is the
    // whole value.
    for p in &paths {
        match Path::new(p).try_exists() {
            Ok(true) => {}
            Ok(false) => {
                eprintln!("itaruby: path not found: {p}");
                return ExitCode::from(2);
            }
            Err(e) => {
                eprintln!("itaruby: cannot read path: {p}: {e}");
                return ExitCode::from(2);
            }
        }
    }

    let (db, sources, discovered) = build_project(&paths);
    announce_discovered_sources(&discovered);

    // bead ita-uo4: `discover_rb_files` walks `read_dir`, so diagnostics
    // came out in whatever order the filesystem handed back — noise in a
    // diff, unstable across machines. Sorting happens HERE, on the print
    // loop only: `sources`/`ProjectFiles` order feeds index construction,
    // so leaving that vector untouched keeps this a rendering fix with no
    // semantic reach.
    let mut to_check: Vec<SourceFile> = sources
        .iter()
        .copied()
        .filter(|f| !discovered.declarations_only.contains(f.path(&db)))
        .collect();
    to_check.sort_by(|a, b| a.path(&db).cmp(b.path(&db)));

    let format_agent = flags.format_agent;
    let format_json = flags.format_json;
    let jobs = check_worker_count(to_check.len());
    let chunk_size = to_check.len().div_ceil(jobs).max(1);
    // bead ita-6dh: compute every file's diagnostics on a worker-thread
    // pool (salsa's `db.clone()`-per-thread idiom — every clone shares
    // the same underlying storage, see salsa's own `tests/parallel/*`),
    // buffer each file's rendered text instead of printing it inline,
    // then print the buffers back in `to_check`'s already-sorted order
    // (ita-uo4) on this thread only. Parallelizes the COMPUTATION;
    // the PRINTED byte stream is exactly what the old sequential loop
    // produced, chunk boundaries are invisible to it.
    let outputs: Vec<(bool, usize, String)> = std::thread::scope(|scope| {
        let handles: Vec<_> = to_check
            .chunks(chunk_size)
            .map(|chunk| {
                let db = db.clone();
                scope.spawn(move || {
                    chunk
                        .iter()
                        .map(|file| {
                            let mut out = String::new();
                            let (err, blocks) =
                                check_and_print(&db, *file, format_agent, format_json, &mut out);
                            (err, blocks, out)
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().expect("check worker panicked"))
            .collect()
    });
    let mut had_error = false;
    let mut agent_blocks = 0usize;
    for (err, blocks, out) in outputs {
        had_error |= err;
        agent_blocks += blocks;
        print!("{out}");
    }
    // Critic gaps (verdict-constraints r1+r2): under `--format=agent`,
    // silence is ambiguous to an unattended agent — "checked and clean"
    // and "silently skipped" look identical, and a run with findings says
    // nothing about the clean files around them. One unconditional
    // summary footer (file count + finding count) disambiguates both
    // without per-file noise at corpus scale; the normal format keeps
    // compiler-style silence on clean code.
    if flags.format_agent {
        println!(
            "_Checked {} file(s); {} constraint finding(s)._",
            to_check.len(),
            agent_blocks,
        );
    }

    if flags.verbose {
        eprintln!(
            "itaruby check: rbi phase 2: parsed {} .rbi files (lazy)",
            itaruby_semantic::rbi_files_parsed_count()
        );
    }
    if flags.stats {
        print_stats(&db, &sources, &discovered.declarations_only);
    }

    if had_error {
        ExitCode::from(1)
    } else {
        ExitCode::from(0)
    }
}

/// Worker count for `run_check`'s parallel per-file diagnostic
/// computation (bead ita-6dh): the machine's core count by default,
/// capped at one worker per file — spawning more threads than there is
/// work just yields empty chunks. `ITARUBY_JOBS` overrides it: NOT a
/// documented `ita check` flag (`parse_check_flags`'s usage string is
/// untouched) — a debug/test knob so the byte-identical-output proof
/// (`tests/multithread_determinism.rs`) can force `1` and diff it
/// against a forced-parallel run from the very same binary.
fn check_worker_count(file_count: usize) -> usize {
    let n = std::env::var("ITARUBY_JOBS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|&n| n > 0)
        .unwrap_or_else(|| std::thread::available_parallelism().map_or(1, std::num::NonZero::get));
    n.min(file_count.max(1))
}

/// One line per discovered declaration source (bead ita-2a9), mirroring
/// `itaruby_server::main_loop::send_discovery_log`'s wording verbatim so
/// `ita check` and `ita server` describe the same discovery the same way
/// in their respective logs. Printed to stderr, unconditionally — no flag
/// gates it, matching `send_discovery_log`'s own default-on behavior —
/// and stdout (both `--format=json` and `--format=agent`'s byte-frozen
/// contracts) is never touched. Unlike the LSP side, which always emits an
/// explicit "no declaration sources found" line to the editor's output
/// panel, total silence here is deliberate: a stray schema/rbi is the
/// exception, and a bare "found nothing" line on every ordinary run would
/// be pure noise on stderr.
fn announce_discovered_sources(discovered: &DiscoveredSources) {
    if let Some(schema) = &discovered.schema_rb {
        eprintln!("itaruby: discovered {} (schema.rb)", schema.display());
    }
    if let Some(sql) = &discovered.structure_sql {
        eprintln!("itaruby: discovered {} (structure.sql)", sql.display());
    }
    if let Some(rbi) = &discovered.rbi_dir {
        eprintln!("itaruby: discovered {} (sorbet/rbi)", rbi.display());
    }
}

/// Load the checked project into a fresh salsa db: walk the given roots
/// for `.rb` sources, run the shared declaration-source discovery
/// (bead ita-16g — db/schema.rb > db/structure.sql, sorbet/rbi), read
/// everything into `ProjectFiles`, then wire `ClosedWorld` (CLI-only, per
/// its contract). Discovery's full `DiscoveredSources` comes back as the
/// third tuple slot: `declarations_only` is what `run_check` excludes
/// from `to_check`/`--stats` exactly as before, and `schema_rb`/
/// `structure_sql`/`rbi_dir` feed `announce_discovered_sources` (bead
/// ita-2a9).
fn build_project(paths: &[String]) -> (Db, Vec<SourceFile>, DiscoveredSources) {
    let mut rb_files = Vec::new();
    for p in paths {
        let root = Path::new(p);
        let ignore_list = sorbet_ignore_entries(root);
        discover_rb_files(root, &ignore_list, &mut rb_files);
    }

    let mut db = Db::new();
    let roots: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
    let discovered = wire_declaration_sources(&mut db, &roots);
    for schema in &discovered.declarations_only {
        if !rb_files.contains(schema) {
            rb_files.push(schema.clone());
        }
    }

    let mut sources = Vec::with_capacity(rb_files.len());
    for path in &rb_files {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(err) => {
                eprintln!("itaruby check: cannot read {}: {err}", path.display());
                continue;
            }
        };
        sources.push(SourceFile::new(&db, path.clone(), text));
    }
    ProjectFiles::new(&db, sources.clone());

    wire_closed_world(&db, paths);

    (db, sources, discovered)
}

/// Check one file, print its diagnostics with excerpts; true when any
/// Error was seen (`run_check`'s exit code depends on it). Severity is
/// always computed for every diagnostic regardless of `format_agent`/
/// `format_json` — the exit code must never depend on which channel is
/// rendered.
///
/// Under `format_agent`, the normal line+excerpt rendering is replaced by
/// a dedicated markdown channel: one self-contained block per E0107
/// contradiction (`diag.constraint`, set by `itaruby_semantic` only on
/// that code — see invariant #1) and one per `Inferred`/`UnionCandidate`
/// outcome from `constraint_report`'s re-walk (the `call_stats` pattern:
/// only `--format=agent` pays for it). Every other diagnostic — the vast
/// majority — is invisible under the flag; this is a channel for the
/// constraint story only, not a markdown-ified version of the normal
/// report. Which outcomes exist and what their candidates are is entirely
/// `itaruby_semantic`'s decision; this function only turns byte offsets
/// into `path:line:col` and lays out headings — zero semantic reach.
///
/// Under `format_json` (contract 2, the ruby-lsp bridge prototype): one
/// `print_json_diagnostic` line per diagnostic, same iteration order as
/// the normal channel (`run_check` already sorts `to_check` by path; this
/// function walks `check_file`'s own order within a file, exactly like
/// the normal render), no excerpt, no footer. `format_agent` wins if both
/// flags are somehow set — mutually exclusive in practice since
/// `--format=` only ever carries one value per invocation.
fn check_and_print(
    db: &Db,
    file: SourceFile,
    format_agent: bool,
    format_json: bool,
    out: &mut String,
) -> (bool, usize) {
    let text = file.text(db);
    let index = LineIndex::new(text);
    let mut had_error = false;
    let mut blocks = 0usize;
    for diag in check_file(db, file) {
        let level = match diag.severity {
            Severity::Error => {
                had_error = true;
                "error"
            }
            Severity::Warning => "warning",
        };
        if format_agent {
            if let Some(proof) = diag.constraint.as_ref() {
                render_contradiction_block(db, file, &index, text, proof, out);
                blocks += 1;
            }
            continue;
        }
        if format_json {
            print_json_diagnostic(db, file, &index, text, diag, level, out);
            continue;
        }
        let (line, col) = index.line_col(text, diag.start);
        writeln!(
            out,
            "{}:{}:{}: {level}[{}]: {}",
            file.path(db).display(),
            line + 1,
            col + 1,
            diag.code,
            diag.message,
        )
        .expect("write to String never fails");
        render_excerpt(db, &index, text, diag, out);
    }
    if format_agent {
        for outcome in constraint_report(db, file) {
            if render_outcome_block(db, file, &index, text, outcome, out) {
                blocks += 1;
            }
        }
    }
    (had_error, blocks)
}

/// Contract 2's JSONL line for one diagnostic: `{path, line, column, code,
/// severity, message}`, 1-based like the normal channel, no excerpt, no
/// did-you-mean/defined-at payload. `serde_json::json!` handles path/message
/// escaping for free.
/// ponytail: `diag.suggestion`/`diag.constraint` are not serialized here —
/// add a `suggestions` field once the ruby-lsp addon actually consumes them.
fn print_json_diagnostic(
    db: &Db,
    file: SourceFile,
    index: &LineIndex,
    text: &str,
    diag: &Diagnostic,
    level: &str,
    out: &mut String,
) {
    let (line, col) = index.line_col(text, diag.start);
    let obj = serde_json::json!({
        "path": file.path(db).display().to_string(),
        "line": line + 1,
        "column": col + 1,
        "code": diag.code,
        "severity": level,
        "message": diag.message,
    });
    writeln!(out, "{obj}").expect("write to String never fails");
}

/// `<path>:<line>:<col>` for one constraint call site, same 1-based
/// `LineIndex` convention `check_and_print`'s normal path uses — the two
/// channels always agree on where a call lives.
fn call_site(
    db: &Db,
    file: SourceFile,
    index: &LineIndex,
    text: &str,
    call: &ConstraintCall,
) -> String {
    let (line, col) = index.line_col(text, call.start);
    format!("{}:{}:{}", file.path(db).display(), line + 1, col + 1)
}

/// One `## constraint contradiction` block per E0107: the proof (every
/// call and its candidates), then 2+ ways to resolve it — fix each
/// conflicting call, or pin the receiver's type with an inline RBS `#:`
/// declaration. Self-contained: an agent acting on this block alone needs
/// nothing else from the report.
fn render_contradiction_block(
    db: &Db,
    file: SourceFile,
    index: &LineIndex,
    text: &str,
    proof: &ConstraintProof,
    out: &mut String,
) {
    writeln!(out, "## constraint contradiction: `{}`", proof.receiver)
        .expect("write to String never fails");
    writeln!(out).expect("write to String never fails");
    writeln!(out, "**Proved**").expect("write to String never fails");
    writeln!(out).expect("write to String never fails");
    for call in &proof.calls {
        writeln!(
            out,
            "- `{}` at {} — candidates: {}",
            call.method,
            call_site(db, file, index, text, call),
            call.candidates.join(", "),
        )
        .expect("write to String never fails");
    }
    writeln!(
        out,
        "- Whichever type `{}` really is, at least one of these calls raises \
    NoMethodError at runtime.",
        proof.receiver,
    )
    .expect("write to String never fails");
    writeln!(
        out,
        "- (Candidates were enumerated from the project's closed-ancestry classes plus \
    Ruby's generated core-class inventory; `open`-ancestry classes — dynamic, reopened, or \
    method_missing — are excluded, so every list above is auditable against those two sources.)"
    )
    .expect("write to String never fails");
    writeln!(out).expect("write to String never fails");
    writeln!(out, "**Resolution**").expect("write to String never fails");
    writeln!(out).expect("write to String never fails");
    for (i, call) in proof.calls.iter().enumerate() {
        writeln!(
            out,
            "{}. Fix `{}` at {} — its candidates ({}) don't overlap with the other call(s).",
            i + 1,
            call.method,
            call_site(db, file, index, text, call),
            call.candidates.join(", "),
        )
        .expect("write to String never fails");
    }
    let example = proof.calls[0]
        .candidates
        .first()
        .map_or("Type", String::as_str);
    writeln!(
        out,
        "{}. Or pin `{}`'s type with an inline RBS declaration — `#: ({example}) -> untyped` \
    on the enclosing `def` line if `{}` is a parameter — picking whichever candidate above is \
    actually correct.",
        proof.calls.len() + 1,
        proof.receiver,
        proof.receiver,
    )
    .expect("write to String never fails");
    writeln!(out).expect("write to String never fails");
}

/// Dispatches an `Inferred`/`UnionCandidate` outcome to
/// `render_declaration_block`; `Contradiction` is skipped here because
/// it's already rendered from `diag.constraint` in the E0107 diagnostic
/// pass — `constraint_report` and the check walk agree on the same proof,
/// so it would otherwise print twice.
fn render_outcome_block(
    db: &Db,
    file: SourceFile,
    index: &LineIndex,
    text: &str,
    outcome: ConstraintOutcome,
    out: &mut String,
) -> bool {
    match outcome {
        ConstraintOutcome::Contradiction(_) => return false,
        ConstraintOutcome::Inferred {
            receiver,
            ty,
            calls,
        } => {
            render_declaration_block(
                &RenderCtx {
                    db,
                    file,
                    index,
                    text,
                },
                &receiver,
                &ty,
                &calls,
                None,
                out,
            );
        }
        ConstraintOutcome::UnionCandidate {
            receiver,
            candidates,
            calls,
        } => {
            let sig_ty = candidates.join(" | ");
            render_declaration_block(
                &RenderCtx {
                    db,
                    file,
                    index,
                    text,
                },
                &receiver,
                &sig_ty,
                &calls,
                Some(candidates.len()),
                out,
            );
        }
    }
    true
}

/// Bundled purely to stay under clippy's argument-count ceiling on
/// `render_declaration_block` — same fields, same borrows as before, no
/// behavior change.
struct RenderCtx<'a> {
    db: &'a Db,
    file: SourceFile,
    index: &'a LineIndex,
    text: &'a str,
}

/// One `## declaration candidate` block: every call that narrowed the
/// receiver plus a ready `#:` line naming `sig_ty`. `ambiguous_of` is
/// `Some(candidate count)` for a `UnionCandidate` outcome (2..=4 types all
/// satisfy every call — genuinely ambiguous, not wrong) and `None` for an
/// `Inferred` outcome (exactly one type does).
fn render_declaration_block(
    ctx: &RenderCtx,
    receiver: &str,
    sig_ty: &str,
    calls: &[ConstraintCall],
    ambiguous_of: Option<usize>,
    out: &mut String,
) {
    writeln!(out, "## declaration candidate: `{receiver}` -> {sig_ty}")
        .expect("write to String never fails");
    writeln!(out).expect("write to String never fails");
    writeln!(out, "**Observed**").expect("write to String never fails");
    writeln!(out).expect("write to String never fails");
    for call in calls {
        writeln!(
            out,
            "- `{}` at {} — candidates: {}",
            call.method,
            call_site(ctx.db, ctx.file, ctx.index, ctx.text, call),
            call.candidates.join(", "),
        )
        .expect("write to String never fails");
    }
    if let Some(n) = ambiguous_of {
        writeln!(
            out,
            "- (Ambiguous: {n} closed candidates satisfy every call above — any of them could \
        be `{receiver}`'s real type.)"
        )
        .expect("write to String never fails");
    }
    writeln!(out).expect("write to String never fails");
    writeln!(out, "**Resolution**").expect("write to String never fails");
    writeln!(out).expect("write to String never fails");
    writeln!(
        out,
        "1. Add an inline RBS declaration for `{receiver}` — `#: ({sig_ty}) -> untyped` on the \
    enclosing `def` line if `{receiver}` is a parameter — replacing `untyped` with the real return \
    type."
    )
    .expect("write to String never fails");
    writeln!(out).expect("write to String never fails");
}

/// Coverage census (`ita check --stats`): per call site, whether the
/// checker actually concluded anything. `inconclusive` + `unknown
/// receiver` is the honest answer to "how much are we NOT checking" —
/// every one of those is invariant #1 buying silence.
///
/// The `receiver unknown` bucket is broken down further by ORIGIN of the
/// `Ty::Unknown` receiver (bead ita-au5) — that breakdown is the gate that
/// decides which inference lever gets built next: heavy `unk_param` /
/// `unk_block_param` argues for call-site parameter inference, heavy
/// `unk_core_ret` argues for filling in official RBS core return types.
/// Each sub-bucket line reads as a share of the whole census (`pct`, same
/// as every other line here), not just a share of `receiver unknown`, so
/// the sub-buckets are directly comparable to `resolved`/`core`/etc.
///
/// The `project ret` sub-bucket is broken down further by CAUSE (bead
/// ita-mv5) — this closes the `[INFERENCE]` the previous bead left open:
/// how much of `project ret` traces back to an untyped parameter, i.e. how
/// much call-site parameter inference (bead ita-djm) would actually
/// unlock. Percentages here are relative to `unk_project_ret`, not to the
/// whole census — the question is "what share of project ret", not "what
/// share of everything".
///
/// The `ancestry open` bucket is broken down by BLOCKER (bead ita-anc): this
/// answers the counterfactual that decides the ancestry work — how much of
/// `ancestry open` is closable project-side versus blocked by an external
/// gem ancestor we would need real RBI knowledge for.
/// `100.0 * part / whole`, shared by every percentage `print_stats` prints.
/// `whole == 0` reads as `0.0` rather than `NaN` — an all-zero census has no
/// fractions to report, and every caller already gates the interesting
/// sub-tables (`project ret by cause`, `ancestry open by blocker`) on their
/// denominator being positive, so this guard is a formality for the
/// top-level census only.
#[expect(
    clippy::cast_precision_loss,
    reason = "part/whole are call-site counts from one checked corpus, orders of magnitude below f64's 2^53 mantissa ceiling — precision loss here is unreachable in practice"
)]
fn pct(part: u64, whole: u64) -> f64 {
    if whole == 0 {
        0.0
    } else {
        part as f64 * 100.0 / whole as f64
    }
}

fn print_stats(db: &Db, sources: &[SourceFile], declarations_only: &[PathBuf]) {
    let mut total = CallStats::default();
    let mut files = 0u64;
    for file in sources
        .iter()
        .filter(|f| !declarations_only.contains(f.path(db)))
    {
        files += 1;
        total.add(&call_stats(db, *file));
    }
    let n = total.total();
    println!("\ncoverage: {files} files, {n} call sites");
    println!("  checked:");
    println!(
        "    project method   {:>8}  {:5.1}%",
        total.resolved,
        pct(total.resolved, n)
    );
    println!(
        "    core method      {:>8}  {:5.1}%",
        total.core,
        pct(total.core, n)
    );
    println!(
        "    rbi method       {:>8}  {:5.1}%",
        total.rbi_method,
        pct(total.rbi_method, n)
    );
    println!(
        "    dsl method       {:>8}  {:5.1}%",
        total.dsl_method,
        pct(total.dsl_method, n)
    );
    println!(
        "    ar api method    {:>8}  {:5.1}%",
        total.ar_api_method,
        pct(total.ar_api_method, n)
    );
    println!(
        "    diagnosed        {:>8}  {:5.1}%",
        total.diagnosed,
        pct(total.diagnosed, n)
    );
    println!("  NOT checked:");
    println!(
        "    ancestry open    {:>8}  {:5.1}%  (receiver class known, ancestry open/incomplete)",
        total.inconclusive,
        pct(total.inconclusive, n)
    );
    println!(
        "    receiver unknown {:>8}  {:5.1}%  (no type for the receiver at all)",
        total.unknown_receiver,
        pct(total.unknown_receiver, n)
    );
    println!(
        "      method param   {:>8}  {:5.1}%  (unrefined parameter)",
        total.unk_param,
        pct(total.unk_param, n)
    );
    println!(
        "      block param    {:>8}  {:5.1}%  (block/yield parameter)",
        total.unk_block_param,
        pct(total.unk_block_param, n)
    );
    println!(
        "      core ret       {:>8}  {:5.1}%  (chain died in a core method with no modeled return)",
        total.unk_core_ret,
        pct(total.unk_core_ret, n)
    );
    println!(
        "      project ret    {:>8}  {:5.1}%  (chain died in a project method with no inferable return)",
        total.unk_project_ret,
        pct(total.unk_project_ret, n)
    );
    println!(
        "      dead chain     {:>8}  {:5.1}%  (receiver already unknown one call earlier)",
        total.unk_chain,
        pct(total.unk_chain, n)
    );
    println!(
        "      ivar           {:>8}  {:5.1}%  (ivar collapsed to Unknown)",
        total.unk_ivar,
        pct(total.unk_ivar, n)
    );
    println!(
        "      constant       {:>8}  {:5.1}%  (unresolved constant)",
        total.unk_const,
        pct(total.unk_const, n)
    );
    println!(
        "      other          {:>8}  {:5.1}%",
        total.unk_other,
        pct(total.unk_other, n)
    );
    if total.unk_project_ret > 0 {
        println!("    project ret by cause:");
        println!(
            "        param            {:>8}  {:5.1}%  (callee's return dies on an untyped parameter)",
            total.ret_param,
            pct(total.ret_param, total.unk_project_ret)
        );
        println!(
            "        ivar             {:>8}  {:5.1}%",
            total.ret_ivar,
            pct(total.ret_ivar, total.unk_project_ret)
        );
        println!(
            "        constant         {:>8}  {:5.1}%",
            total.ret_const,
            pct(total.ret_const, total.unk_project_ret)
        );
        println!(
            "        explicit return  {:>8}  {:5.1}%",
            total.ret_explicit,
            pct(total.ret_explicit, total.unk_project_ret)
        );
        println!(
            "        other            {:>8}  {:5.1}%",
            total.ret_other,
            pct(total.ret_other, total.unk_project_ret)
        );
        println!(
            "        callee unresolved{:>8}  {:5.1}%",
            total.ret_unresolved,
            pct(total.ret_unresolved, total.unk_project_ret)
        );
    }
    if total.inconclusive > 0 {
        println!("    ancestry open by blocker:");
        println!(
            "        unresolved name   {:>8}  {:5.1}%  (ancestor name we cannot resolve at all)",
            total.anc_unresolved,
            pct(total.anc_unresolved, total.inconclusive)
        );
        println!(
            "        declared open     {:>8}  {:5.1}%  (gems.rbi entry declared but carrying no methods)",
            total.anc_declared,
            pct(total.anc_declared, total.inconclusive)
        );
        if total.not_ar_api > 0 {
            println!(
                "          of which ActiveRecord::Base, unreachable {:>8}  {:5.1}%  (declared open, blocked by ActiveRecord::Base, callee matches neither AR API set — generated attribute/association or client method; real API calls already escalated to `ar api method` above)",
                total.not_ar_api,
                pct(total.not_ar_api, total.anc_declared)
            );
        }
        println!(
            "        project dsl       {:>8}  {:5.1}%  (unmodeled class-body call)",
            total.anc_dsl,
            pct(total.anc_dsl, total.inconclusive)
        );
        println!(
            "        project block     {:>8}  {:5.1}%  (block on a class-body call: `included do`)",
            total.anc_block,
            pct(total.anc_block, total.inconclusive)
        );
        println!(
            "        project meta      {:>8}  {:5.1}%  (eval/send/dynamic define)",
            total.anc_meta,
            pct(total.anc_meta, total.inconclusive)
        );
        println!(
            "        project missing   {:>8}  {:5.1}%  (method_missing / abstract raise)",
            total.anc_missing,
            pct(total.anc_missing, total.inconclusive)
        );
        println!(
            "        project other     {:>8}  {:5.1}%",
            total.anc_other,
            pct(total.anc_other, total.inconclusive)
        );
        println!(
            "        not ancestry      {:>8}  {:5.1}%",
            total.anc_na,
            pct(total.anc_na, total.inconclusive)
        );
    }
    println!(
        "    blind total      {:>8}  {:5.1}%",
        total.blind(),
        pct(total.blind(), n)
    );
}

/// rustc-compact excerpt under a diagnostic's primary line (w12 closure):
/// the source line, then a caret line aligned under the diagnostic span,
/// with the diagnostic's did-you-mean and defined-at payload appended.
/// Columns count in chars (not bytes or UTF-16) so the caret lands on the
/// offending symbol even with multibyte source; tabs print as-is —
/// ponytail: expand tabs if real-world excerpts ever misalign. Omits
/// nothing, invents nothing: `suggestion` is exactly what the checker
/// attached.
fn render_excerpt(db: &Db, index: &LineIndex, text: &str, diag: &Diagnostic, out: &mut String) {
    let line = index.line_col(text, diag.start).0;
    let line_start = index.line_start(line);
    let line_end = text[line_start..]
        .find('\n')
        .map_or(text.len(), |i| line_start + i);
    let src = text[line_start..line_end].trim_end_matches('\r');
    let span_start = diag.start.min(line_end);
    let span_end = diag.end.max(span_start).min(line_end);
    let col = text[line_start..span_start].chars().count();
    let carets = "^".repeat(text[span_start..span_end].chars().count().max(1));
    let ln = (line + 1).to_string();
    let pad = " ".repeat(ln.len());
    writeln!(out, "{ln} | {src}").expect("write to String never fails");
    let indent = " ".repeat(col);
    let mut caret_line = format!("{pad} | {indent}{carets}");
    if let Some(s) = &diag.suggestion {
        write!(caret_line, " did you mean `{}`?", s.name).expect("write to String never fails");
    }
    writeln!(out, "{caret_line}").expect("write to String never fails");
    if let Some((file, start)) = diag.suggestion.as_ref().and_then(|s| s.def) {
        let site_text = file.text(db);
        let (dl, _) = LineIndex::new(site_text).line_col(site_text, start);
        writeln!(
            out,
            "{pad} | {indent}defined at {}:{}",
            file.path(db).display(),
            dl + 1
        )
        .expect("write to String never fails");
    }
}

/// bead ita-9d4: `ignore_list` is `sorbet/config`'s `--ignore` entries,
/// root-relative (see `sorbet_ignore_entries`). Empty for every call site
/// except `build_project`'s (`ita check`'s own root) — `run_definition`/
/// `run_hover` pass `&[]` and keep their pre-existing, unfiltered walk;
/// only the checked command's own discovery honors the project's own
/// ignore list.
fn discover_rb_files(root: &Path, ignore_list: &[String], out: &mut Vec<PathBuf>) {
    if root.is_file() {
        if root.extension().and_then(|e| e.to_str()) == Some("rb") {
            out.push(root.to_path_buf());
        }
        return;
    }
    for entry in ignore::WalkBuilder::new(root).build() {
        let Ok(entry) = entry else { continue };
        let is_file = entry.file_type().is_some_and(|t| t.is_file());
        let is_rb = entry.path().extension().and_then(|e| e.to_str()) == Some("rb");
        if !is_file || !is_rb {
            continue;
        }
        if !ignore_list.is_empty() {
            if let Ok(rel) = entry.path().strip_prefix(root) {
                if path_is_ignored(rel, ignore_list) {
                    continue;
                }
            }
        }
        out.push(entry.path().to_path_buf());
    }
}

/// bead ita-9d4: sorbet's own `sorbet/config` can list `--ignore=<path>`
/// entries (also written as the two-token `--ignore <path>` form,
/// argv-style — sorbet's config file is one flag/value per line) that
/// exclude a subtree from typechecking. Honoring it removes a source of
/// noise-only diagnostics from paths the project itself declared out of
/// scope (vendored code, generated fixtures, ...).
///
/// LSP is explicitly OUT of scope: an editor already knows which files
/// are open and sends exactly those; `--ignore` is a check-TIME concept
/// about which files enter a typechecking run in the first place, not
/// something `ita server`'s per-file requests need to consult. This is
/// the documented ceiling of this bead — `ita check` only, never wired
/// into `itaruby_server::main_loop`'s own `discover_rb_files`.
///
/// No `sorbet/config` (missing, unreadable, or simply empty of `--ignore`
/// lines) returns `vec![]`: the anti-overreach control
/// (`tests/sorbet_ignore_cli.rs`'s absent-config test) leans on this —
/// an empty list makes `discover_rb_files`'s filtering a pure no-op, so
/// "no config" and "old behavior" are the same code path, not merely
/// close to it.
fn sorbet_ignore_entries(root: &Path) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(root.join("sorbet").join("config")) else {
        return Vec::new();
    };
    let mut tokens = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'));
    let mut entries = Vec::new();
    while let Some(tok) = tokens.next() {
        if let Some(path) = tok.strip_prefix("--ignore=") {
            entries.push(normalize_ignore_entry(path));
        } else if tok == "--ignore" {
            if let Some(path) = tokens.next() {
                entries.push(normalize_ignore_entry(path));
            }
        }
    }
    entries
}

/// Strips one trailing `/` so `--ignore=test/fixtures/` and
/// `--ignore=test/fixtures` (I2: "without trailing slash still excludes
/// the directory it names") normalize to the same entry before
/// `path_is_ignored` compares path components.
fn normalize_ignore_entry(path: &str) -> String {
    path.strip_suffix('/').unwrap_or(path).to_string()
}

/// Component-boundary match (I2) for one discovered file's root-relative
/// path against `sorbet/config`'s `--ignore` entries: entry `test/fix`
/// must NOT exclude `test/fixtures2/x.rb`. Comparing `Path::components()`
/// rather than raw string prefixes gets this right for free — `test/fix`
/// is `["test", "fix"]`, which is not a prefix of `test/fixtures2/x.rb`'s
/// `["test", "fixtures2", "x.rb"]`, whereas `test/fixtures` (`["test",
/// "fixtures"]`) IS a prefix of `["test", "fixtures", "x.rb"]` — so it
/// excludes the directory it names whether or not it had a trailing `/`
/// (already stripped by `normalize_ignore_entry`).
fn path_is_ignored(rel: &Path, ignore_list: &[String]) -> bool {
    let rel_components: Vec<_> = rel.components().collect();
    ignore_list.iter().any(|entry| {
        let entry_components: Vec<_> = Path::new(entry).components().collect();
        !entry_components.is_empty() && rel_components.starts_with(&entry_components)
    })
}

/// bead ita-2ve: same 4-level upward walk as `find_upward`, but for the
/// three shapes that mean "this project has gems" — a `Gemfile`, a
/// `Gemfile.lock`, or any `*.gemspec` sitting directly in a candidate
/// directory. Discovery is per checked root and only ever looks UPWARD:
/// a `Gemfile` nested deeper than a root (e.g. inside a `testdata/`
/// subdirectory while checking `testdata/` itself) does not turn the
/// mode off — checking that subdirectory as its own root does.
///
/// The start is resolved (`upward_start`), and a root we cannot resolve
/// reports gems PRESENT: this mode exists to be off whenever the gem set
/// is not provably known, and "I could not look" is not knowledge.
/// Reading an unresolvable root as gemless is how a symlinked `app/` turned
/// 2 real errors into 310 fabricated ones (`tests/symlinked_root_cli.rs`).
/// ponytail: that fail-closed branch has no test — reaching it needs a root
/// that passes `try_exists` and then fails `canonicalize`, which the CLI
/// guard makes a race, not a fixture. Add one if a real report produces it.
fn gems_detected(root: &Path) -> bool {
    let Some(mut dir) = itaruby_semantic::upward_start(root) else {
        return true;
    };
    for _ in 0..4 {
        if dir.join("Gemfile").is_file() || dir.join("Gemfile.lock").is_file() {
            return true;
        }
        if let Ok(entries) = std::fs::read_dir(&dir) {
            if entries
                .filter_map(Result::ok)
                .any(|e| e.path().extension().is_some_and(|x| x == "gemspec") && e.path().is_file())
            {
                return true;
            }
        }
        let Some(parent) = dir.parent() else {
            return false;
        };
        dir = parent.to_path_buf();
    }
    false
}

/// Same 4-level upward walk as `find_upward_dir`, for a bare file name
/// sitting directly in a candidate directory (w12 closure:
/// `Gemfile.lock`). `gems_detected` deliberately does NOT reuse this —
/// it also needs the `*.gemspec` glob and its own per-shape semantics.
fn find_upward_file(root: &Path, name: &str) -> Option<PathBuf> {
    let mut dir = itaruby_semantic::upward_start(root)?;
    for _ in 0..4 {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
        dir = dir.parent()?.to_path_buf();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real-world shapes, including one private benchmark corpus's multi-source lock (two GEM
    /// sections), 6-space dependency constraints that must NOT count,
    /// platform-suffixed versions, and the non-GEM sections that end the
    /// harvest.
    #[test]
    fn parses_gemfile_lock_gem_specs() {
        let lock = "\
GEM
  remote: https://gems.contribsys.com/
  specs:
    sidekiq-pro (7.3.2)
      sidekiq (>= 7.3.0, < 8)

GEM
  remote: https://rubygems.org/
  specs:
    activesupport (7.1.3)
      concurrent-ruby (~> 1.0)
    nokogiri (1.15.0-arm64-darwin)
    my_gem (2.0.0)

PLATFORMS
  ruby
  arm64-darwin-22

DEPENDENCIES
  sidekiq-pro (~> 7.3)

PATH
  local-thing

BUNDLED WITH
   2.4.10
";
        assert_eq!(
            parse_gemfile_lock_gems(lock),
            ["sidekiq-pro", "activesupport", "nokogiri", "my_gem"]
        );
    }

    #[test]
    fn no_gem_section_or_empty_lock_yields_no_gems() {
        assert!(parse_gemfile_lock_gems("PLATFORMS\n  ruby\n").is_empty());
        assert!(parse_gemfile_lock_gems("").is_empty());
        // GEM section without a specs block (remote-only) yields nothing.
        assert!(parse_gemfile_lock_gems("GEM\n  remote: https://x/\n").is_empty());
    }
}
