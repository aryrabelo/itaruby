//! W11: core-receiver E0101 in a Bundler project, licensed only by
//! evidence that every gem's additions to the core method chain are known.
//!
//! The closed-world contract (bead ita-2ve) accuses an unknown method on a
//! core receiver only when nothing can reopen the class. A `Gemfile` used
//! to end that: any gem might give `Integer` the method. A project that ran
//! `tapioca gems` holds runtime-reflected evidence of what every gem
//! defines. This module decides whether that evidence is COMPLETE, and
//! owns the two inventories the open-world lookup reads besides the RBIs:
//! the stdlib's core extensions and the source-audited gems Tapioca never
//! writes an RBI for. Every unknown fails closed to the old silence.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

/// Gems Tapioca never writes an RBI for by its own policy
/// (`Tapioca::Gemfile::GemSpec::IGNORED_GEMS`: sorbet, sorbet-static,
/// sorbet-runtime, sorbet-static-and-runtime, debug, irb; `fakefs` is not
/// here and stays a hole), with the exact versions whose source was
/// audited and the instance methods each adds to the core receiver chain
/// (`Integer`/`Float`/`String`/`Symbol`/`Array`/`Hash`/`NilClass`/
/// `TrueClass`/`FalseClass` and their `Numeric`/`Comparable`/
/// `Enumerable`/`Object`/`Kernel`/`BasicObject` ancestry). A locked
/// version not listed here is unaudited and fails closed.
///
/// Audit (2026-10-20, re-run 2026-11-19 after the cubic review hardened
/// `scripts/audit-curated-gems.rb`, over each version's unpacked `lib/`
/// and every `.rb` or Ruby-shebang file under `exe/`, aborting on a parse
/// error): every `class`/`module` reopening of a chain name, every
/// chain-receiver `include`/`prepend`/`define_method`/`send`/
/// `class_eval`/`instance_exec`, every `alias` keyword, every top-level
/// `def`/`alias`/mixin (a private or public `Object` method), every
/// `refine`, every `eval` whatever its receiver, `class << self` bodies
/// labeled singleton, and every `is_a?`/`kind_of?`/`instance_of?`/
/// `method_missing` definition. Findings:
/// - sorbet-static, sorbet-static-and-runtime: no Ruby under `lib/`.
/// - sorbet-runtime: no chain reopening. Its one type-test override,
///   `T::InterfaceWrapper#is_a?` (0.5.10262 only), answers true for its
///   own class and an interface MODULE, never for a core class.
/// - sorbet (the `srb` CLI, never required by an application):
///   `DelegateClass` at top level and its top-level `alias`
///   `DelegateClass_without_rbi_generator`, `Object.prepend` of `extend`,
///   and `Kernel.send(:define_method, ...)` of `exit`/`at_exit`.
/// - debug: `Kernel#debugger` and its `alias bb` (under
///   `RUBY_DEBUG_BB`), `Kernel` prepends of `fork`/`_fork`/`trap`/
///   `daemon`, `BasicObject#singleton_method_added`.
/// - irb: top-level `xmp`, `irb_binding` defined on `Object` by a
///   string `eval` in `IRB::WorkSpace`, and the two `Object` aliases of
///   `irb/ext/use-loader.rb` (`__original__load__IRB_use_loader__`,
///   `__original__require__IRB_use_loader__`). Its other `eval`s run
///   user or debugger expressions and define no name.
pub const CURATED_GEMS: &[(&str, &[&str], &[&str])] = &[
    ("sorbet-static", SORBET_VERSIONS, &[]),
    ("sorbet-static-and-runtime", SORBET_VERSIONS, &[]),
    ("sorbet-runtime", SORBET_VERSIONS, &[]),
    (
        "sorbet",
        SORBET_VERSIONS,
        &[
            "DelegateClass",
            "DelegateClass_without_rbi_generator",
            "extend",
            "exit",
            "at_exit",
        ],
    ),
    (
        "debug",
        &["1.11.1"],
        &[
            "debugger",
            "bb",
            "fork",
            "_fork",
            "trap",
            "daemon",
            "singleton_method_added",
        ],
    ),
    (
        "irb",
        &["1.15.2", "1.16.0", "1.17.0", "1.18.0"],
        &[
            "xmp",
            "irb_binding",
            "__original__load__IRB_use_loader__",
            "__original__require__IRB_use_loader__",
        ],
    ),
];

const SORBET_VERSIONS: &[&str] = &[
    "0.5.10262",
    "0.5.12443",
    "0.6.13055",
    "0.6.13184",
    "0.6.13437",
    "0.6.13508",
    "0.6.13544",
    "0.6.13558",
    "0.6.13571",
];

/// What a complete lock proved: the chain names its audited gems add, and
/// what the installed source of every chain-touching gem adds (`Harvest`).
#[derive(Debug, Default)]
pub struct LockEvidence {
    pub curated_names: HashSet<String>,
    pub harvest: Harvest,
}

/// Where `lock_evidence` looks besides the lock: the project's Tapioca
/// `sorbet/rbi/gems` directory and the directories gems are installed in.
pub struct GemEnv<'a> {
    pub rbi_versions: &'a HashMap<String, Vec<String>>,
    pub rbi_gems_dir: &'a Path,
    pub gem_roots: &'a [PathBuf],
}

/// What a set of Ruby files adds to the core receiver chain, NAME-KEYED
/// (AGENTS.md: when the receiver cannot be seen, only the name carries
/// the proof):
/// - `names`: chain namespace -> every method name a readable reopening,
///   literal `define_method`/`alias_method`/`attr_*`, or resolved mixin
///   adds to it;
/// - `any_names`: names defined through a receiver no constant names,
///   which may land on any class;
/// - `open`: chain namespaces whose additions cannot be read (an eval
///   body, a dynamic `define_method`, a class-body block, an unresolved
///   mixin). `BasicObject` open means every chain is open;
/// - `type_test`: some non-chain class overrides `is_a?`/`kind_of?`/
///   `instance_of?` or answers `method_missing`, so a declared type is
///   not exact.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Harvest {
    pub names: HashMap<String, HashSet<String>>,
    pub any_names: HashSet<String>,
    pub open: HashSet<String>,
    pub type_test: bool,
}

impl Harvest {
    pub fn merge(&mut self, other: Self) {
        for (ns, names) in other.names {
            self.names.entry(ns).or_default().extend(names);
        }
        self.any_names.extend(other.any_names);
        self.open.extend(other.open);
        self.type_test |= other.type_test;
    }

    fn open_all(&mut self) {
        self.open.insert("BasicObject".to_string());
    }
}

/// Is every gem a `Gemfile.lock` installs accounted for?
///
/// - `GEM` and `GIT` specs need a Tapioca RBI for the LOCKED version
///   (`sorbet/rbi/gems/<name>@<version>[-<suffix>].rbi`; a stale RBI
///   describes another version and does not count), or a `CURATED_GEMS`
///   entry naming that version.
/// - `PATH` specs must live inside the project: their source is then the
///   project's own, read by the checker or by `unloaded_core_hazard`.
/// - Any other section that declares specs (`PLUGIN SOURCE`) is
///   unreadable.
///
/// The text must also be a lock Bundler wrote: every section header is
/// one Bundler writes, every line of a source section has one of its
/// shapes, and `PLATFORMS` and `DEPENDENCIES` (written even for a Gemfile
/// with no gems) are present. A lock with no specs then proves zero gems;
/// an empty or garbled file proves nothing, because Bundler resolves the
/// `Gemfile` again at runtime and loads whatever it names.
///
/// # Errors
///
/// The first gap, named; any `Err` keeps the world open.
pub fn lock_evidence(
    lock_text: &str,
    lock_dir: &Path,
    env: &GemEnv<'_>,
) -> Result<LockEvidence, String> {
    let mut evidence = LockEvidence::default();
    let mut state = LockState::default();
    for line in lock_text.lines() {
        if line.is_empty() {
            continue;
        }
        if !line.starts_with(' ') {
            state.enter_section(line)?;
            continue;
        }
        if !SOURCE_SECTIONS.contains(&state.section) {
            continue;
        }
        let Some((name, version)) = state.source_line(line)? else {
            continue;
        };
        let spec = Spec {
            section: state.section,
            remote: state.remote,
            name,
            version,
        };
        account_spec(&spec, lock_dir, env, &mut evidence)?;
    }
    if !(state.platforms && state.dependencies) {
        return Err("Gemfile.lock lacks PLATFORMS or DEPENDENCIES: not a lock Bundler wrote".into());
    }
    Ok(evidence)
}

/// Where `lock_evidence` is in the lock, and which of Bundler's
/// always-written sections it has seen.
#[derive(Default)]
struct LockState<'a> {
    section: &'a str,
    remote: Option<&'a str>,
    platforms: bool,
    dependencies: bool,
}

impl<'a> LockState<'a> {
    /// A non-indented line: one of Bundler's section headers, or a gap.
    fn enter_section(&mut self, line: &'a str) -> Result<(), String> {
        self.section = line.trim_end();
        self.remote = None;
        match self.section {
            "PLATFORMS" => self.platforms = true,
            "DEPENDENCIES" => self.dependencies = true,
            s if LOCK_SECTIONS.contains(&s) => {}
            other => return Err(format!("Gemfile.lock has an unread section `{other}`")),
        }
        Ok(())
    }

    /// One line of a source section: the spec it declares (`name`,
    /// `version`), nothing for a `key:` or dependency-constraint line, or
    /// a gap for a line of no Bundler shape.
    fn source_line(&mut self, line: &'a str) -> Result<Option<(&'a str, &'a str)>, String> {
        if let Some(r) = line.strip_prefix("  remote: ") {
            self.remote = Some(r.trim());
            return Ok(None);
        }
        let Some(rest) = line.strip_prefix("    ") else {
            if !is_source_key(line) {
                return Err(format!("unreadable lock line `{line}`"));
            }
            return Ok(None);
        };
        if rest.starts_with(' ') {
            return Ok(None);
        }
        parse_spec(rest)
            .map(Some)
            .ok_or_else(|| format!("unreadable spec line `{rest}`"))
    }
}

/// The section headers Bundler writes; specs are read only in the
/// `SOURCE_SECTIONS`, the others carry no gem.
const LOCK_SECTIONS: &[&str] = &[
    "GEM",
    "GIT",
    "PATH",
    "PLUGIN SOURCE",
    "PLATFORMS",
    "DEPENDENCIES",
    "RUBY VERSION",
    "BUNDLED WITH",
    "CHECKSUMS",
];
const SOURCE_SECTIONS: &[&str] = &["GEM", "GIT", "PATH", "PLUGIN SOURCE"];

/// A source section's two-space `key:` line (`remote:`, `revision:`,
/// `specs:`, `glob:`, ...).
fn is_source_key(line: &str) -> bool {
    line.strip_prefix("  ")
        .and_then(|rest| rest.split_once(':'))
        .is_some_and(|(key, _)| !key.is_empty() && key.chars().all(|c| c.is_ascii_lowercase() || c == '_'))
}

/// One lock spec and where it was read.
struct Spec<'a> {
    section: &'a str,
    remote: Option<&'a str>,
    name: &'a str,
    version: &'a str,
}

/// A `GEM`/`GIT` spec: an RBI for the locked version (whose chain
/// reopenings are then read from the installed source, or left open per
/// class without one), a source-audited Tapioca-ignored version, or an
/// installed pure-Ruby source read for everything.
fn account_gem(
    name: &str,
    version: &str,
    env: &GemEnv<'_>,
    evidence: &mut LockEvidence,
) -> Result<(), String> {
    if rbi_covers(env.rbi_versions, name, version) {
        let touched = rbi_chain_touch(env.rbi_gems_dir, name, version);
        if !touched.is_empty() {
            match installed_gem_dir(env.gem_roots, name, version) {
                Some(dir) => match gem_files(&dir) {
                    Ok(files) => evidence.harvest.merge(harvest_gem(&files, Reach::Targeted)),
                    Err(_) => evidence.harvest.open_all(),
                },
                None => evidence.harvest.open.extend(touched),
            }
        }
    } else if let Some(names) = curated(name, version) {
        evidence
            .curated_names
            .extend(names.iter().map(|n| (*n).to_string()));
    } else {
        let dir = installed_gem_dir(env.gem_roots, name, version).ok_or_else(|| {
            format!("gem `{name} ({version})` has no RBI and no installed source for its locked version")
        })?;
        let files = gem_files(&dir).map_err(|path| {
            format!("gem `{name} ({version})` has no RBI and {} cannot be read", path.display())
        })?;
        if native_extension(&dir, &files) {
            return Err(format!(
                "gem `{name} ({version})` has no RBI and a native extension"
            ));
        }
        evidence.harvest.merge(harvest_gem(&files, Reach::Everything));
    }
    Ok(())
}

/// `lock_evidence`'s per-spec rule; `Err` names the gap.
///
/// Tapioca's RBI is runtime reflection, and it is NOT complete for core
/// reopenings: measured, `json_expressions` 0.9.0 does `alias_method
/// :ignore_extra_keys!, :forgiving!` inside `class Hash`, and its RBI
/// (and `hidden.rbi`) lists only the `include`. So a gem whose RBI
/// reopens a chain namespace is read from its INSTALLED SOURCE for the
/// locked version (`harvest_gem`); with no source on disk, every chain
/// namespace that RBI touches stays open, and only those - a gem that
/// only reopens `Hash` cannot add a method to `Integer`. A gem with no
/// RBI for its locked version (stale, excluded, never generated) needs
/// either a `CURATED_GEMS` audit or its installed source; nothing else
/// says where it might reach, so a missing source is a gap.
fn account_spec(
    spec: &Spec<'_>,
    lock_dir: &Path,
    env: &GemEnv<'_>,
    evidence: &mut LockEvidence,
) -> Result<(), String> {
    let Spec {
        section,
        remote,
        name,
        version,
    } = *spec;
    match section {
        "GEM" | "GIT" => account_gem(name, version, env, evidence)?,
        "PATH" => {
            let path = remote.ok_or_else(|| format!("PATH gem `{name}` has no remote"))?;
            if !inside(lock_dir, &lock_dir.join(path)) {
                return Err(format!("PATH gem `{name}` lives outside the project"));
            }
        }
        other => return Err(format!("spec `{name}` in unread section `{other}`")),
    }
    Ok(())
}

/// `name (version[-platform])` -> (`name`, `version`). The platform suffix
/// is cut: Tapioca names the RBI after the version alone.
fn parse_spec(rest: &str) -> Option<(&str, &str)> {
    let open = rest.find(" (")?;
    let name = &rest[..open];
    let inner = rest[open + 2..].strip_suffix(')')?;
    let version = inner.split('-').next()?;
    let valid = |s: &str| {
        !s.is_empty()
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
    };
    (valid(name) && valid(version)).then_some((name, version))
}

fn rbi_covers<S: std::hash::BuildHasher>(
    rbi_versions: &HashMap<String, Vec<String>, S>,
    name: &str,
    version: &str,
) -> bool {
    rbi_versions.get(name).is_some_and(|versions| {
        versions.iter().any(|v| {
            v.strip_prefix(version)
                .is_some_and(|tail| tail.is_empty() || tail.starts_with('-'))
        })
    })
}

fn curated(name: &str, version: &str) -> Option<&'static [&'static str]> {
    CURATED_GEMS
        .iter()
        .find(|(gem, versions, _)| *gem == name && versions.contains(&version))
        .map(|(_, _, names)| *names)
}

fn inside(root: &Path, path: &Path) -> bool {
    match (std::fs::canonicalize(root), std::fs::canonicalize(path)) {
        (Ok(root), Ok(path)) => path.starts_with(root),
        _ => false,
    }
}

/// `gem name -> every RBI version` in a `sorbet/rbi/gems` directory:
/// `<name>@<version>[-<suffix>].rbi` split at the `@` (gem names never
/// contain one).
pub fn rbi_gem_versions(gems_dir: &Path) -> HashMap<String, Vec<String>> {
    let mut out: HashMap<String, Vec<String>> = HashMap::new();
    let Ok(entries) = std::fs::read_dir(gems_dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("rbi") {
            continue;
        }
        if let Some((name, version)) = path
            .file_stem()
            .and_then(|s| s.to_str())
            .and_then(|s| s.split_once('@'))
        {
            out.entry(name.to_string())
                .or_default()
                .push(version.to_string());
        }
    }
    out
}

/// What the Ruby files under `project` that this run does NOT load add to
/// the core receiver chain. Every `.rb`/`.rake`/`.ru`/`.gemspec`/
/// `Gemfile`/`Rakefile`/`bin/*` file outside `loaded` is read by the same
/// walker the index uses and harvested name-keyed (`harvest_files`).
/// Gitignore does not hide a file here - an ignored file still runs - so
/// only `.git` and the Bundler install path (installed gems, covered by
/// the lock) are skipped.
///
/// # Errors
///
/// An unreadable file, directory (or project) is named: nothing says what
/// it adds, and a walk that skipped it is not a complete read.
pub fn harvest_unloaded<S: std::hash::BuildHasher>(
    project: &Path,
    loaded: &HashSet<PathBuf, S>,
) -> Result<Harvest, PathBuf> {
    let Ok(project) = std::fs::canonicalize(project) else {
        return Err(project.to_path_buf());
    };
    let skip = bundle_paths(&project);
    let walker = ignore::WalkBuilder::new(&project)
        .standard_filters(false)
        .filter_entry(move |entry| {
            entry.file_name() != ".git" && !skip.iter().any(|dir| entry.path() == dir)
        })
        .build();
    let mut files = Vec::new();
    for entry in walker {
        let entry = entry.map_err(|err| walk_error_path(&err, &project))?;
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let path = entry.into_path();
        if crate::discovery::ruby_source(&path) && !loaded.contains(&path) {
            files.push(path);
        }
    }
    harvest_files(&files, Reach::Everything)
}

/// The installed source of `name` at exactly `version` under any of
/// `roots` (`<root>/<name>-<version>` or `<name>-<version>-<platform>`).
/// A git checkout (`bundler/gems/<name>-<sha>`) is never matched: its
/// revision is not the version string, so it stays a gap.
pub fn installed_gem_dir(roots: &[PathBuf], name: &str, version: &str) -> Option<PathBuf> {
    let exact = format!("{name}-{version}");
    let platform = format!("{exact}-");
    roots.iter().find_map(|root| {
        std::fs::read_dir(root).ok()?.flatten().find_map(|entry| {
            let file = entry.file_name();
            let file = file.to_str()?;
            (file == exact || file.starts_with(&platform)).then(|| entry.path())
        })
    })
}

/// Does an installed gem carry compiled code (an `ext/` tree or a built
/// `.so`/`.bundle`)? C defines methods no Ruby walker can read. `files`
/// is the gem's complete listing (`gem_files`).
fn native_extension(dir: &Path, files: &[PathBuf]) -> bool {
    dir.join("ext").is_dir()
        || files.iter().any(|p| {
            matches!(
                p.extension().and_then(|x| x.to_str()),
                Some("so" | "bundle" | "dll")
            )
        })
}

/// Every file of an installed gem, the WHOLE gem: a gemspec's
/// `require_paths` may name any directory besides `lib/`, and a `lib/`
/// file may `require_relative` outside it. Symlinks are followed (a loop
/// is an error).
///
/// # Errors
///
/// The entry the walk could not read: a partial listing is not a clean
/// gem.
fn gem_files(dir: &Path) -> Result<Vec<PathBuf>, PathBuf> {
    let mut out = Vec::new();
    for entry in ignore::WalkBuilder::new(dir)
        .standard_filters(false)
        .follow_links(true)
        .build()
    {
        let entry = entry.map_err(|err| walk_error_path(&err, dir))?;
        if entry.path().is_file() {
            out.push(entry.into_path());
        }
    }
    Ok(out)
}

/// The path a walk error names, else the walk's root.
fn walk_error_path(err: &ignore::Error, root: &Path) -> PathBuf {
    match err {
        ignore::Error::WithPath { path, .. } => path.clone(),
        ignore::Error::WithDepth { err, .. } | ignore::Error::WithLineNumber { err, .. } => {
            walk_error_path(err, root)
        }
        ignore::Error::Loop { child, .. } => child.clone(),
        _ => root.to_path_buf(),
    }
}

/// Every chain namespace the gem's Tapioca RBI for the locked version
/// reopens (`rbi::scan_chain_header`). An unreadable RBI touches every
/// chain.
fn rbi_chain_touch(gems_dir: &Path, name: &str, version: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    let Ok(entries) = std::fs::read_dir(gems_dir) else {
        out.insert("BasicObject".to_string());
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some((n, v)) = path
            .file_stem()
            .and_then(|s| s.to_str())
            .and_then(|s| s.split_once('@'))
        else {
            continue;
        };
        let tail_ok = v
            .strip_prefix(version)
            .is_some_and(|t| t.is_empty() || t.starts_with('-'));
        if n != name || !tail_ok {
            continue;
        }
        match std::fs::read_to_string(&path) {
            Ok(text) => out.extend(
                text.lines()
                    .filter_map(crate::rbi::scan_chain_header)
                    .map(str::to_string),
            ),
            Err(_) => {
                out.insert("BasicObject".to_string());
            }
        }
    }
    out
}

/// Harvest an installed gem from its complete listing (`gem_files`):
/// every `.rb` file. An unreadable file opens every chain.
pub fn harvest_gem(files: &[PathBuf], reach: Reach) -> Harvest {
    let rb: Vec<PathBuf> = files
        .iter()
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("rb"))
        .cloned()
        .collect();
    harvest_files(&rb, reach).unwrap_or_else(|_| {
        let mut h = Harvest::default();
        h.open_all();
        h
    })
}

/// How far a harvest must reach.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reach {
    /// A gem with a Tapioca RBI for its locked version. The RBI is the
    /// gem's reflected surface after loading, so whatever its code
    /// defined at load time through a receiver no constant names, and any
    /// type-test override, is already there (`rbi_core_declares`,
    /// `rbi_type_test_hazard`). What the RBI measurably drops is what a
    /// chain REOPENING adds (`alias_method` in `class Hash`); only the
    /// chain-targeted shapes are read: reopening fragments, keyed sources
    /// on a chain constant, the blanket `core_mixin`, and the two shapes
    /// whose reach nothing bounds - a refinement with an unknown target
    /// (no RBI reflects a refinement; `using` activates it) and an eval
    /// body with an unknown target - which open every chain.
    Targeted,
    /// A gem with no RBI, or a project file this run does not load:
    /// nothing else describes it, so every shape counts.
    Everything,
}

/// Name-keyed harvest of `files`, read as ONE unit so a mixin defined in
/// one file and included into a core class in another resolves.
///
/// # Errors
///
/// The first unreadable file.
pub fn harvest_files(files: &[PathBuf], reach: Reach) -> Result<Harvest, PathBuf> {
    let mut all = Vec::with_capacity(files.len());
    for path in files {
        let text = std::fs::read_to_string(path).map_err(|_| path.clone())?;
        all.push(crate::index::parse_defs_text(&text));
    }
    let modules = module_table(&all);
    let mut h = Harvest::default();
    for defs in &all {
        harvest_defs(defs, &modules, reach, &mut h);
    }
    Ok(h)
}

/// A module's readable instance surface in the harvested unit (its own
/// names plus the modules it mixes in), or `None` when some fragment of
/// it cannot be read.
type ModuleTable = HashMap<String, Option<(HashSet<String>, Vec<String>)>>;

fn module_table(all: &[crate::FileDefs]) -> ModuleTable {
    let mut out: ModuleTable = HashMap::new();
    for frag in all.iter().flat_map(|d| d.fragments.iter()) {
        let key = frag.path.trim_start_matches("::").to_string();
        let readable = fragment_readable(frag);
        let slot = out
            .entry(key)
            .or_insert_with(|| Some((HashSet::new(), Vec::new())));
        match (slot.as_mut(), readable) {
            (Some((names, mixins)), true) => {
                names.extend(frag.methods.iter().map(|m| m.name.clone()));
                mixins.extend(frag.includes.iter().chain(&frag.prepends).cloned());
            }
            _ => *slot = None,
        }
    }
    out
}

/// A fragment whose method list is its whole addition: never opened, or
/// opened only because it reopens a class defined elsewhere.
fn fragment_readable(frag: &crate::index::ClassFragment) -> bool {
    !frag.body_unreadable
        && (!frag.open || frag.open_reason == Some(crate::index::OpenReason::ReopenedExternal))
}

/// Every instance name module `path` hands an includer: from the
/// harvested unit (transitively), else the stdlib's top-level module
/// surface. `None`: unknown, fail closed.
fn module_names(modules: &ModuleTable, path: &str, depth: usize) -> Option<HashSet<String>> {
    let key = path.trim_start_matches("::");
    if depth > 8 {
        return None;
    }
    match modules.get(key) {
        Some(Some((names, mixins))) => {
            let mut out = names.clone();
            for m in mixins {
                out.extend(module_names(modules, m, depth + 1)?);
            }
            Some(out)
        }
        Some(None) => None,
        None => STDLIB_MODULES
            .get(key)
            .map(|names| names.iter().map(|n| (*n).to_string()).collect()),
    }
}

/// The chain namespace a written target names, if any.
fn chain_target(target: &str) -> Option<String> {
    let t = target.trim_start_matches("::");
    is_chain_name(t).then(|| t.to_string())
}

/// One file's additions, name-keyed. The blanket `core_mixin` flag is set
/// by the walker only for what it cannot key (a toplevel
/// `method_missing`); the keyed injections arrive through
/// `keyed_pollution`.
fn harvest_defs(defs: &crate::FileDefs, modules: &ModuleTable, reach: Reach, h: &mut Harvest) {
    let untargeted = reach == Reach::Everything;
    harvest_blanket(defs, h);
    if untargeted {
        harvest_untargeted(defs, modules, h);
    }
    for (target, _, source) in &defs.keyed_pollution {
        harvest_keyed(target.as_deref(), source, modules, untargeted, h);
    }
    for frag in &defs.fragments {
        harvest_fragment(frag, modules, untargeted, h);
    }
}

/// The file-level marks that name no method: a `method_missing` the
/// walker could not bound, unknown refinement/eval targets (for every
/// reach, see `Reach::Targeted`), and every refined or evaled chain
/// namespace.
fn harvest_blanket(defs: &crate::FileDefs, h: &mut Harvest) {
    if defs.core_mixin {
        h.open_all();
    }
    if defs.refined_unknown || defs.eval_unknown {
        h.open_all();
    }
    for (target, _) in defs.refine_targets.iter().chain(&defs.eval_targets) {
        if let Some(ns) = chain_target(target) {
            h.open.insert(ns);
        }
    }
}

/// What a pollution source can add, by name; `None` is unreadable.
fn source_names(
    source: &crate::index::PollutionSource,
    modules: &ModuleTable,
) -> Option<Vec<String>> {
    use crate::index::PollutionSource;
    match source {
        PollutionSource::Names(n) => Some(n.clone()),
        PollutionSource::Singleton(_) => Some(Vec::new()),
        PollutionSource::Opaque => None,
        PollutionSource::Module(m) => module_names(modules, m, 0).map(|s| s.into_iter().collect()),
    }
}

/// Definitions and mixins through receivers no constant names: they may
/// land on any class.
fn harvest_untargeted(defs: &crate::FileDefs, modules: &ModuleTable, h: &mut Harvest) {
    for (_, source) in &defs.hidden_definitions {
        match source_names(source, modules) {
            Some(n) => h.any_names.extend(n),
            None => h.open_all(),
        }
    }
    for (_, module, _) in &defs.dynamic_mixin_targets {
        match module_names(modules, module, 0) {
            Some(n) => h.any_names.extend(n),
            None => h.open_all(),
        }
    }
}

/// One name-keyed source: on a chain namespace it adds its names there
/// (or opens it); without a target it reaches every class, read only
/// for an untargeted harvest.
fn harvest_keyed(
    target: Option<&str>,
    source: &crate::index::PollutionSource,
    modules: &ModuleTable,
    untargeted: bool,
    h: &mut Harvest,
) {
    let ns = match target {
        None if !untargeted => return,
        None => None,
        Some(t) => match chain_target(t) {
            Some(ns) => Some(ns),
            None => return,
        },
    };
    match (source_names(source, modules), ns) {
        (Some(n), Some(ns)) => h.names.entry(ns).or_default().extend(n),
        (Some(n), None) => h.any_names.extend(n),
        (None, Some(ns)) => {
            h.open.insert(ns);
        }
        (None, None) => h.open_all(),
    }
}

fn harvest_fragment(
    frag: &crate::index::ClassFragment,
    modules: &ModuleTable,
    untargeted: bool,
    h: &mut Harvest,
) {
    // A type-test override on ANY class, a chain class included (`class
    // String; def is_a?`), lets a value pass sorbet-runtime's `is_a?`
    // check for a type it is not.
    h.type_test |= untargeted
        && frag.methods.iter().any(|m| {
            crate::index::TYPE_TEST_NAMES.contains(&m.name.as_str())
                || matches!(m.name.as_str(), "method_missing" | "respond_to_missing?")
        });
    let Some(ns) = chain_target(&frag.path) else {
        return;
    };
    if !fragment_readable(frag) {
        h.open.insert(ns);
        return;
    }
    let mut names: HashSet<String> = frag.methods.iter().map(|m| m.name.clone()).collect();
    for m in frag.includes.iter().chain(&frag.prepends) {
        if is_core_include(&ns, m.trim_start_matches("::")) {
            continue;
        }
        if let Some(n) = module_names(modules, m, 0) {
            names.extend(n);
        } else {
            h.open.insert(ns.clone());
        }
    }
    h.names.entry(ns).or_default().extend(names);
}

/// The directories Bundler installs gems into under `project`:
/// `vendor/bundle` and `.bundle/config`'s `BUNDLE_PATH`.
fn bundle_paths(project: &Path) -> Vec<PathBuf> {
    let mut out = vec![project.join("vendor/bundle")];
    if let Ok(config) = std::fs::read_to_string(project.join(".bundle/config")) {
        for line in config.lines() {
            if let Some(value) = line.trim().strip_prefix("BUNDLE_PATH:") {
                let value = value.trim().trim_matches(['"', '\'']);
                if !value.is_empty() {
                    out.push(project.join(value));
                }
            }
        }
    }
    out
}

/// `module -> public instance names` for each top-level module the
/// standard library defines (`@Module#name` lines of
/// `declarations/stdlib_core_ext.txt`; `@Module` alone: empty surface).
static STDLIB_MODULES: LazyLock<HashMap<&'static str, HashSet<&'static str>>> =
    LazyLock::new(|| {
        let mut out: HashMap<&str, HashSet<&str>> = HashMap::new();
        for line in include_str!("../declarations/stdlib_core_ext.txt").lines() {
            let Some(rest) = line.strip_prefix('@') else {
                continue;
            };
            match rest.split_once('#') {
                Some((module, name)) => {
                    out.entry(module).or_default().insert(name);
                }
                None => {
                    out.entry(rest).or_default();
                }
            }
        }
        out
    });

/// Is `module` already in `owner`'s ancestry in MRI itself (`String`
/// includes `Comparable`, `Object` includes `Kernel`, ...)? Tapioca
/// writes those inclusions into every reopening it emits; restating a
/// core fact adds nothing the chain lookup does not already read, while
/// any OTHER inclusion of a chain module (`class Integer; include
/// Enumerable`) would hand the receiver names the inventory never lists,
/// and stays unreadable.
pub fn is_core_include(owner: &str, module: &str) -> bool {
    matches!(
        (owner, module),
        ("Object", "Kernel")
            | (
                "String" | "Symbol" | "Numeric" | "Integer" | "Float",
                "Comparable"
            )
            | ("Array" | "Hash", "Enumerable")
    )
}

/// A namespace on some core receiver's method chain.
pub fn is_chain_name(name: &str) -> bool {
    crate::core::is_core_namespace(name) || name == "BasicObject"
}

/// `class -> names` the standard library adds to a core receiver class
/// when a program requires it (`declarations/stdlib_core_ext.txt`,
/// generated by `scripts/gen-stdlib-core-ext.rb`). `*` means a feature
/// added `method_missing`/`respond_to_missing?`.
static STDLIB_CORE_EXT: LazyLock<HashMap<&'static str, HashSet<&'static str>>> =
    LazyLock::new(|| {
        let mut out: HashMap<&str, HashSet<&str>> = HashMap::new();
        for line in include_str!("../declarations/stdlib_core_ext.txt").lines() {
            if line.starts_with(['#', '@']) {
                continue;
            }
            if let Some((class, name)) = line.split_once('#') {
                out.entry(class).or_default().insert(name);
            }
        }
        out
    });

/// Does a standard-library feature give an instance of `class` the method
/// `name`? `Object` and `BasicObject` lines reach every class.
pub fn stdlib_core_ext_has(class: &str, name: &str) -> bool {
    [class, "Object", "BasicObject"].iter().any(|c| {
        STDLIB_CORE_EXT
            .get(c)
            .is_some_and(|names| names.contains(name) || names.contains("*"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(lock: &str, versions: &HashMap<String, Vec<String>>) -> Result<LockEvidence, String> {
        let env = GemEnv {
            rbi_versions: versions,
            rbi_gems_dir: Path::new("/nonexistent-w11"),
            gem_roots: &[],
        };
        lock_evidence(lock, Path::new("."), &env)
    }

    fn rbis(pairs: &[(&str, &str)]) -> HashMap<String, Vec<String>> {
        let mut out: HashMap<String, Vec<String>> = HashMap::new();
        for (n, v) in pairs {
            out.entry((*n).to_string())
                .or_default()
                .push((*v).to_string());
        }
        out
    }

    const LOCK: &str = "GEM\n  remote: https://rubygems.org/\n  specs:\n    nokogiri (1.19.3-arm64-darwin)\n      racc (~> 1.4)\n    racc (1.8.1)\n    sorbet-runtime (0.6.13437)\n\nPLATFORMS\n  ruby\n\nDEPENDENCIES\n  nokogiri\n";

    #[test]
    fn complete_lock_with_curated_runtime() {
        let found =
            ev(LOCK, &rbis(&[("nokogiri", "1.19.3"), ("racc", "1.8.1")])).expect("complete");
        assert!(found.curated_names.is_empty());
    }

    #[test]
    fn stale_rbi_version_is_a_gap() {
        let err = ev(LOCK, &rbis(&[("nokogiri", "1.19.2"), ("racc", "1.8.1")]));
        assert!(err.unwrap_err().contains("nokogiri"));
    }

    #[test]
    fn unaudited_curated_version_is_a_gap() {
        let lock = LOCK.replace("0.6.13437", "0.6.99999");
        let err = ev(&lock, &rbis(&[("nokogiri", "1.19.3"), ("racc", "1.8.1")]));
        assert!(err.unwrap_err().contains("sorbet-runtime"));
    }

    /// What Bundler writes after the source sections of every lock.
    const TAIL: &str = "\nPLATFORMS\n  ruby\n\nDEPENDENCIES\n  mygem!\n";

    #[test]
    fn plugin_source_specs_are_unreadable() {
        let lock = format!("PLUGIN SOURCE\n  remote: https://x\n  specs:\n    mygem (1.0)\n{TAIL}");
        assert!(ev(&lock, &HashMap::new()).unwrap_err().contains("PLUGIN SOURCE"));
    }

    #[test]
    fn git_rbi_with_sha_suffix_covers() {
        let lock = format!(
            "GIT\n  remote: https://x\n  revision: abc\n  specs:\n    json_api_client (1.23.0)\n{TAIL}"
        );
        assert!(ev(&lock, &rbis(&[("json_api_client", "1.23.0-606196")])).is_ok());
        assert!(ev(&lock, &rbis(&[("json_api_client", "1.23.01")])).is_err());
    }

    /// A Gemfile with no gems locks to sources with no specs: zero gems,
    /// complete. An empty or structureless file is not a lock Bundler
    /// wrote, so it proves nothing.
    #[test]
    fn only_a_lock_bundler_wrote_proves_zero_gems() {
        let empty_gems = "GEM\n  remote: https://rubygems.org/\n  specs:\n\nPLATFORMS\n  ruby\n\nDEPENDENCIES\n\nBUNDLED WITH\n   2.5.0\n";
        assert!(ev(empty_gems, &HashMap::new()).is_ok());
        for garbled in [
            "",
            "\n\n",
            "GEM\n  remote: https://rubygems.org/\n  specs:\n",
            "GEM\n  remote: https://rubygems.org/\n  specs:\n\tmygem (1.0.0)\n\nPLATFORMS\n  ruby\n\nDEPENDENCIES\n  mygem\n",
            "GEM\n  remote: https://rubygems.org/\n  specs:\n   mygem (1.0.0)\n\nPLATFORMS\n  ruby\n\nDEPENDENCIES\n  mygem\n",
            "<<<<<<< HEAD\nGEM\n  specs:\n\nPLATFORMS\n  ruby\n\nDEPENDENCIES\n",
        ] {
            assert!(ev(garbled, &HashMap::new()).is_err(), "must not prove zero gems: {garbled:?}");
        }
    }

    #[test]
    fn stdlib_extension_is_known() {
        assert!(stdlib_core_ext_has("Integer", "to_json"));
        assert!(stdlib_core_ext_has("String", "shellescape"));
        assert!(!stdlib_core_ext_has("Integer", "upcase"));
    }

    /// Two GEM sections (a private source plus rubygems), 6-space
    /// dependency lines that are constraints, not gems, a platform suffix,
    /// and the sections that end a source.
    #[test]
    fn every_gem_section_counts_and_constraints_do_not() {
        let lock = "GEM\n  remote: https://gems.contribsys.com/\n  specs:\n    sidekiq-pro (7.3.2)\n      sidekiq (>= 7.3.0, < 8)\n\nGEM\n  remote: https://rubygems.org/\n  specs:\n    activesupport (7.1.3)\n      concurrent-ruby (~> 1.0)\n    nokogiri (1.15.0-arm64-darwin)\n\nPLATFORMS\n  ruby\n\nDEPENDENCIES\n  sidekiq-pro (~> 7.3)\n\nBUNDLED WITH\n   2.4.10\n";
        let all = rbis(&[
            ("sidekiq-pro", "7.3.2"),
            ("activesupport", "7.1.3"),
            ("nokogiri", "1.15.0"),
        ]);
        assert!(ev(lock, &all).is_ok());
        let without_private = rbis(&[("activesupport", "7.1.3"), ("nokogiri", "1.15.0")]);
        assert!(ev(lock, &without_private)
            .unwrap_err()
            .contains("sidekiq-pro"));
    }
}
