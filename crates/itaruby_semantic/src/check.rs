//! Flow-sensitive body checking. Invariant #1: `Ty::Unknown` (and `Union`)
//! receivers/arguments never produce diagnostics.

use std::collections::{HashMap, HashSet};

use rustc_hash::{FxHashMap, FxHashSet};

use itaruby_syntax::ruby_prism::{self, CallNode, DefNode, Node, Visit};
use itaruby_syntax::SourceFile;

use crate::core::{
    core_block_keeps_lexical_self, core_class_of, core_const_ty, core_inventory_has,
    core_inventory_names, core_method, core_pollution_names, is_known_core_constant,
    kernel_bare_call_method, arg_pollution_keys, core_own_names, CoreClass, CoreRet,
};
use crate::index::{
    const_path_str, file_defs, project_index, Blocker, MethodLookup, MethodSig, OpenReason,
    ProjectIndex,
};
use crate::rbs_comment::{parse_cast_comment, CastTarget, RbsParam, RbsSig, RbsTy};
use crate::schema::cast_risk;
use crate::types::{
    ClassId, ConstraintCall, ConstraintProof, Diagnostic, Severity, Suggestion, Ty,
    E0001_SYNTAX_ERROR, E0101_UNKNOWN_METHOD, E0102_WRONG_ARITY, E0103_ARG_TYPE_MISMATCH,
    E0104_UNRESOLVED_CONSTANT, E0105_INVALID_RBS_COMMENT, E0106_IMPOSSIBLE_CAST,
    E0107_CONSTRAINT_CONTRADICTION, E0108_OPERAND_TYPE_MISMATCH, E0109_RETURN_TYPE_MISMATCH,
};

type Env = FxHashMap<String, Ty>;
type KeywordArg = (String, Ty, (usize, usize));
type PositionalArg = (Ty, (usize, usize), Option<String>);

struct CallTypeArgs<'a> {
    positional: &'a [PositionalArg],
    /// None means splats/forwarding/blocks made named binding unprovable.
    keywords: Option<&'a [KeywordArg]>,
    /// Spans of the arguments WRITTEN as a literal `nil` — the only nil a
    /// Sorbet contract may accuse (see `contract_accuses`).
    nil_literals: &'a [(usize, usize)],
}

/// Bead ita-qst: every `#: as <target>` inline-cast comment
/// (sorbet.org/docs/rbs-support's "inline type assertion" form) in
/// `text`, keyed by the BYTE RANGE OF ITS OWN PHYSICAL LINE rather than a
/// line number — lets `Checker::cast_comment_at` bind a comment to
/// whichever argument node's own span falls on that same physical line,
/// with nothing fancier than two `str::find` calls. Line-binding is the
/// documented ceiling, same as `index.rs`'s `sig_comments` map: a call
/// whose several arguments share one physical line is out of scope, and
/// two casts on the same line would collide — neither shape appears in
/// the RBS-support corpus this bead targets (one argument per line is
/// exactly how ruby-lsp/Tapioca emit this form; see `handle_change` in
/// ruby-lsp's own `index.rb`).
fn collect_cast_comments(
    text: &str,
    comments: impl Iterator<Item = (usize, usize)>,
) -> Vec<((usize, usize), CastTarget)> {
    let mut out = Vec::new();
    for (start, end) in comments {
        let ctext = text[start..end].trim_end();
        let Some(body) = ctext.strip_prefix("#:") else {
            continue;
        };
        let Some(target) = parse_cast_comment(body.trim()) else {
            continue;
        };
        let line_start = text[..start].rfind('\n').map_or(0, |i| i + 1);
        let line_end = text[end..].find('\n').map_or(text.len(), |i| end + i);
        out.push(((line_start, line_end), target));
    }
    out
}

/// Bead ita-k9j, entrega 2: mechanically harvested `ActiveRecord::Base`
/// API surface (`declarations/activerecord_api.txt`, generator versioned
/// at `scripts/gen-activerecord-inventory.rs`) — a THIRD escalation
/// source for `Checker::rbi_escalate`, tried last, after the DSL RBI and
/// gem RBI walks. Unlike those two, this source needs no client
/// `sorbet/rbi` at all (see `rbi_escalate`'s doc comment): it fires just
/// as well on a project with zero Tapioca coverage as on one with full
/// coverage — the additive-only argument entrega 1 (`ita-xze`) already
/// made for `gems.rbi` applies unchanged here. `Checker::tally_ar_base`
/// still consults these same sets, but only for the remainder
/// `rbi_escalate` already missed (see that function's doc comment). Same
/// embedding pattern as `core.rs`'s `CORE_INVENTORY`/`declarations/
/// gems.rbi` — `include_str!`, never a path discovered at runtime.
const AR_API: &str = include_str!("../declarations/activerecord_api.txt");

/// `ActiveRecord::Base#name` lines from `AR_API`, name only.
static AR_API_INSTANCE: std::sync::LazyLock<HashSet<&'static str>> = std::sync::LazyLock::new(|| {
    AR_API
        .lines()
        .filter_map(|l| l.strip_prefix("ActiveRecord::Base#"))
        .collect()
});

/// `ActiveRecord::Base.name` lines from `AR_API`, name only.
static AR_API_SINGLETON: std::sync::LazyLock<HashSet<&'static str>> = std::sync::LazyLock::new(|| {
    AR_API
        .lines()
        .filter_map(|l| l.strip_prefix("ActiveRecord::Base."))
        .collect()
});

/// One class-object call-site verdict from the dark singleton census.
/// Measurement only: this type never becomes a diagnostic, and the census
/// reads nothing the walk does not already read, so `check_file_dark`'s
/// diagnostic vec is byte-identical to `check_file`'s.
#[derive(Debug, Clone)]
pub struct DarkSingleton {
    pub start: usize,
    pub end: usize,
    /// The receiver's index path (e.g. `Page`).
    pub receiver: String,
    pub method: String,
    pub verdict: DarkVerdict,
}

#[derive(Debug, Clone)]
pub enum DarkVerdict {
    /// The singleton chain is provably closed (complete, no declared
    /// external, no open ancestor) and the lookup is unsoftened, and the
    /// name is absent — the residue: every site bucketed here is a
    /// prospective E0101 if the class-object track ever arms.
    ClosedNotFound,
    /// A blocker stands the receiver down (`index::inconclusive_reason`'s
    /// census reason, Debug-formatted) — silence is the correct verdict and
    /// the reason names what an inventory bead would have to model.
    Open(String),
    /// Bead ita-tail: silent because the NAME is the core bare-call tail
    /// (`raise`, `rand`, ... — `core::kernel_bare_call_method`). Name-keyed
    /// label, deterministic and auditable; it may overlap another core
    /// softener (`extend` is both `Object#extend` and tail), and it never
    /// becomes a diagnostic. The residue (`ClosedNotFound`) excludes these.
    KnownTail,
}

/// All diagnostics for one file: parse errors, invalid sigs, body checks.
#[salsa::tracked]
pub fn check_file(db: &dyn salsa::Database, file: SourceFile) -> Vec<Diagnostic> {
    check_file_inner(db, file, false).0
}

/// Dark singleton census (measurement instrument, never a gate): the exact
/// walk `check_file` runs with the `Ty::Class` call arms recording their
/// lookup verdict. Deliberately NOT salsa-tracked — a census is run-scoped,
/// and caching a side channel would let one call order silently drop every
/// record (the tracked wrapper above stays the LSP/CLI diagnostic path).
pub fn check_file_dark(
    db: &dyn salsa::Database,
    file: SourceFile,
) -> (Vec<Diagnostic>, Vec<DarkSingleton>) {
    check_file_inner(db, file, true)
}

fn check_file_inner(
    db: &dyn salsa::Database,
    file: SourceFile,
    dark: bool,
) -> (Vec<Diagnostic>, Vec<DarkSingleton>) {
    let text = file.text(db);
    // RBI files are declarations, not executable source bodies.
    if file.path(db).extension().is_some_and(|ext| ext == "rbi") {
        return (Vec::new(), Vec::new());
    }
    let parse = ruby_prism::parse(text.as_bytes());
    let cast_comments = collect_cast_comments(
        text,
        parse.comments().map(|c| {
            let loc = c.location();
            (loc.start_offset(), loc.end_offset())
        }),
    );

    let mut diags = Vec::new();
    for err in parse.errors() {
        let loc = err.location();
        diags.push(Diagnostic {
            start: loc.start_offset(),
            end: loc.end_offset(),
            code: E0001_SYNTAX_ERROR,
            severity: Severity::Error,
            message: format!("syntax error: {}", err.message()),
            suggestion: None,
            constraint: None,
        });
    }
    let defs = file_defs(db, file);
    for (start, end, msg) in &defs.sig_errors {
        diags.push(Diagnostic {
            start: *start,
            end: *end,
            code: E0105_INVALID_RBS_COMMENT,
            severity: Severity::Warning,
            message: format!("invalid RBS signature comment: {msg}"),
            suggestion: None,
            constraint: None,
        });
    }

    let index = project_index(db);
    let rbi_map = crate::RbiProject::try_get(db).map(|r| r.map(db));
    let mut checker = Checker {
        db,
        index,
        rbi_map,
        cast_comments,
        diags,
        silent: false,
        dark: false,
        dark_recs: Vec::new(),
        returns: Vec::new(),
        return_memo: FxHashMap::default(),
        contract_memo: std::cell::RefCell::default(),
        return_cause_memo: FxHashMap::default(),
        in_progress: FxHashSet::default(),
        depth: 0,
        goto_target: None,
        goto_found: None,
        hover_target: None,
        hover_ty: None,
        hover_method: None,
        current_method_name: None,
        ivar_capture: None,
        ivar_class_memo: FxHashMap::default(),
        ivar_class_in_progress: FxHashSet::default(),
        stats: CallStats::default(),
        constraints: HashMap::new(),
        collect_constraint_outcomes: false,
        constraint_outcomes: Vec::new(),
        census: false,
        method_params: Vec::new(),
        block_params: Vec::new(),
        rebindable_block_depth: 0,
        local_origin: FxHashMap::default(),
        unknown_call_src: FxHashMap::default(),
        last_ret_cause: None,
        defined_guards: Vec::new(),
        respond_to_guards: Vec::new(),
        narrowed_names: Vec::new(),
        asserted_subject_spans: Vec::new(),
        operand_locals: FxHashMap::default(),
    };
    let mut env = Env::default();
    // Toplevel statements are one Ruby local scope; E0108's literal
    // proof is per scope (see `Checker::operand_locals`).
    checker.operand_locals = checker.scope_operand_locals(None, Some(&parse.node()));
    checker.dark = dark;
    checker.walk_scope(&[], None, false, &parse.node(), &mut env);
    let dark_recs = std::mem::take(&mut checker.dark_recs);
    let mut out = checker.diags;
    out.sort_by_key(|d| (d.start, d.code));
    (out, dark_recs)
}

/// A resolved call-site definition: which file, and the byte span of the
/// target method's name.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct DefSite {
    pub file: SourceFile,
    pub start: usize,
    pub end: usize,
}

/// Coverage census (one entry per call site in the walked file): which
/// resolution outcome the checker actually reached. `inconclusive` and
/// `unknown_receiver` are the blind spots — call sites where invariant #1
/// buys silence, i.e. what we are NOT checking.
#[derive(Default, Clone, Copy, Debug)]
pub struct CallStats {
    /// Resolved to a project method (arity/sig/return all checked).
    pub resolved: u64,
    /// Resolved against a core-class method.
    pub core: u64,
    /// Bead ita-xze/ita-uh1: `Inconclusive` for a project-class receiver,
    /// but an EXTERNAL ancestor (unresolved name, or a
    /// `declarations/gems.rbi` force-open stub) declares the method in
    /// the client's Tapioca RBIs (`index::rbi_method_lookup`). Conclusive
    /// — no diagnostic either way, since `Inconclusive` never diagnosed
    /// before ita-xze — but carved OUT of `inconclusive` below, same
    /// discipline as every other top-level bucket: the sum below plus
    /// this one is `total()`. No sig rides along a hit, so arity is
    /// never checked, but the call DOES now type with the sig's mapped
    /// return `Ty` (ita-uh1) — see `Checker::rbi_escalate`'s doc comment.
    pub rbi_method: u64,
    /// Bead ita-tjr: same shape as `rbi_method`, but the hit came from a
    /// Tapioca DSL RBI (`sorbet/rbi/dsl/`) that reopens the RECEIVER'S
    /// OWN project class (or a project ancestor of it) — not an external
    /// gem ancestor. Checked FIRST in `rbi_escalate`, since it is the
    /// more specific of the two populations; `rbi_method` only counts a
    /// hit that fell through this bucket. Disjoint from `rbi_method` by
    /// construction: a call site increments exactly one of the two, so
    /// `total()` still sums every top-level bucket exactly once.
    pub dsl_method: u64,
    /// Bead ita-k9j (entrega 2): same shape as `rbi_method`/`dsl_method`,
    /// but the hit came from `declarations/activerecord_api.txt` —
    /// itaruby's own curated `ActiveRecord::Base` API name list, never a
    /// client RBI. Tried LAST in `rbi_escalate`, after both RBI-backed
    /// sources, because it is the coarsest of the three: it knows only
    /// names, never types, so every hit types `Ty::Unknown` (no sig
    /// rides along, so arity is never checked — same contract as the
    /// other two). Disjoint from `rbi_method`/`dsl_method` by
    /// construction: a call site increments exactly one of the three.
    pub ar_api_method: u64,
    /// Receiver class known, but its ancestry is open or incomplete: no
    /// conclusion possible.
    pub inconclusive: u64,
    /// Receiver type itself unknown (or a union): nothing was checked.
    pub unknown_receiver: u64,
    /// Concluded the method does not exist — an emitted E0101.
    pub diagnosed: u64,
    /// Bead ita-au5: breakdown of `unknown_receiver` by the ORIGIN of the
    /// `Ty::Unknown` receiver. Each of the 8 fields below counts a
    /// disjoint subset of `unknown_receiver`'s call sites; their sum
    /// always equals `unknown_receiver` (asserted by
    /// `tests/call_stats.rs`). Measurement only — never a diagnostic.
    /// Receiver is an unrefined method parameter.
    pub unk_param: u64,
    /// Receiver is a block/yield parameter.
    pub unk_block_param: u64,
    /// Chain died in a core method with no modeled return.
    pub unk_core_ret: u64,
    /// Chain died in a project method with no inferable return.
    pub unk_project_ret: u64,
    /// Receiver is a call whose OWN receiver was already unknown.
    pub unk_chain: u64,
    /// Receiver is an ivar that collapsed to Unknown.
    pub unk_ivar: u64,
    /// Receiver is an unresolved constant.
    pub unk_const: u64,
    /// Anything else.
    pub unk_other: u64,
    /// Bead ita-mv5: breakdown of `unk_project_ret` by the CAUSE the
    /// callee's own inferred return died on `Ty::Unknown` — the question
    /// bead ita-djm (call-site parameter inference) needs answered before
    /// it can measure its own payoff. Each of the 6 fields below counts a
    /// disjoint subset of `unk_project_ret`'s call sites; their sum
    /// always equals `unk_project_ret` (asserted by `tests/ret_cause.rs`).
    /// Measurement only — never a diagnostic.
    /// Callee's return died on its own method/block parameter.
    pub ret_param: u64,
    /// Callee's return died on an ivar that collapsed to Unknown.
    pub ret_ivar: u64,
    /// Callee's return died on an unresolved constant.
    pub ret_const: u64,
    /// Folded return Unknown via an explicit `return`, tail itself not Unknown.
    pub ret_explicit: u64,
    /// Any other / unclassifiable cause.
    pub ret_other: u64,
    /// Project-ret site with no callee at all (`Inconclusive`/`NotFound`).
    pub ret_unresolved: u64,
    /// Bead ita-anc: breakdown of `inconclusive` by whether the blocking
    /// `open`/incomplete ancestor is inside the project (closable without
    /// gem knowledge) or declared/unresolved external (needs Tapioca
    /// RBI). Each of the 6 fields below counts a disjoint subset of
    /// `inconclusive`'s call sites; their sum always equals
    /// `inconclusive` (asserted by `tests/ancestry_census.rs`).
    /// Measurement only — never a diagnostic, never flips an `open` flag.
    /// Chain blocked by an ancestor NAME that never resolved — we do not
    /// know what the ancestor even is. Needs name resolution (more
    /// declarations / RBI), and it survives the `anc_declared` fix.
    pub anc_unresolved: u64,
    /// Chain blocked by an ancestor we DID resolve, to a
    /// `declarations/gems.rbi` entry that is force-open because the
    /// declaration carries no methods. Closable by giving that one name a
    /// real method set — no new name resolution required. Split from
    /// `anc_unresolved` (bead ita-o1n) because they are different fixes.
    pub anc_declared: u64,
    /// Project ancestor opened by an unmodeled class-body DSL call (the
    /// `_ =>` catch-all): `validates`, `belongs_to`, `scope`. Closable by
    /// modeling what those calls generate, or by an allowlist of the ones
    /// that generate nothing.
    pub anc_dsl: u64,
    /// Project ancestor opened by a block attached to a class-body call —
    /// `included do ... end`, `FIELDS.each do`. Split from `anc_dsl`
    /// (bead ita-o1n) because it is a different fix: the block body must
    /// be walked in class scope.
    pub anc_block: u64,
    /// Project ancestor opened by a dynamic/metaprogramming construct
    /// (dynamic superclass, dynamic mixin, `class_eval`/`send`, ...).
    pub anc_meta: u64,
    /// Project ancestor opened by `method_missing` or the abstract-raise
    /// idiom.
    pub anc_missing: u64,
    /// Project ancestor open for any other reason.
    pub anc_other: u64,
    /// Inconclusive call site whose receiver ancestry is NOT the cause
    /// (e.g. the concrete-core receiver miss arm) — not an ancestry
    /// block at all.
    pub anc_na: u64,
    /// Bead ita-k9j: of the `anc_declared` call sites `inconclusive_reason`
    /// still reaches — i.e. `rbi_escalate`'s `ar_api_method` step above
    /// ALSO missed this exact call site (see `Checker::tally_ar_base`'s
    /// doc comment for why that miss is guaranteed by the time this
    /// counts) — how many are blocked specifically by a declared-external
    /// `ActiveRecord::Base` ancestor (`ProjectIndex::declared_by`) whose
    /// callee name is in NEITHER of `declarations/activerecord_api.txt`'s
    /// sets: a generated attribute/association, or a client-defined
    /// method — the genuinely unreachable remainder no static declaration
    /// can name. Measurement only — never a diagnostic, never flips an
    /// `open` flag.
    pub not_ar_api: u64,
}

impl CallStats {
    pub fn total(&self) -> u64 {
        self.resolved
            + self.core
            + self.rbi_method
            + self.dsl_method
            + self.ar_api_method
            + self.inconclusive
            + self.unknown_receiver
            + self.diagnosed
    }

    /// Call sites where nothing was concluded.
    pub fn blind(&self) -> u64 {
        self.inconclusive + self.unknown_receiver
    }

    pub fn add(&mut self, o: &CallStats) {
        self.resolved += o.resolved;
        self.core += o.core;
        self.rbi_method += o.rbi_method;
        self.dsl_method += o.dsl_method;
        self.ar_api_method += o.ar_api_method;
        self.inconclusive += o.inconclusive;
        self.unknown_receiver += o.unknown_receiver;
        self.diagnosed += o.diagnosed;
        self.unk_param += o.unk_param;
        self.unk_block_param += o.unk_block_param;
        self.unk_core_ret += o.unk_core_ret;
        self.unk_project_ret += o.unk_project_ret;
        self.unk_chain += o.unk_chain;
        self.unk_ivar += o.unk_ivar;
        self.unk_const += o.unk_const;
        self.unk_other += o.unk_other;
        self.ret_param += o.ret_param;
        self.ret_ivar += o.ret_ivar;
        self.ret_const += o.ret_const;
        self.ret_explicit += o.ret_explicit;
        self.ret_other += o.ret_other;
        self.ret_unresolved += o.ret_unresolved;
        self.anc_unresolved += o.anc_unresolved;
        self.anc_declared += o.anc_declared;
        self.anc_dsl += o.anc_dsl;
        self.anc_block += o.anc_block;
        self.anc_meta += o.anc_meta;
        self.anc_missing += o.anc_missing;
        self.anc_other += o.anc_other;
        self.anc_na += o.anc_na;
        self.not_ar_api += o.not_ar_api;
    }
}

#[derive(Copy, Clone)]
enum Bucket {
    Resolved,
    Core,
    RbiMethod,
    DslMethod,
    ArApiMethod,
    Inconclusive,
    UnknownReceiver,
    Diagnosed,
}

/// Origin classification of a `Ty::Unknown` receiver (bead ita-au5). See
/// `Checker::unknown_origin`'s doc comment for how each variant is
/// derived; `CallStats`'s `unk_*` fields count these 1:1.
#[derive(Copy, Clone, PartialEq, Eq)]
enum UnkOrigin {
    Param,
    BlockParam,
    CoreRet,
    ProjectRet,
    Chain,
    Ivar,
    Const,
    Other,
}

/// Cause of an `unk_project_ret` call site's blindness (bead ita-mv5): why
/// the callee's own inferred return died on `Ty::Unknown`. `ret_cause_of`
/// maps `UnkOrigin::Param`/`UnkOrigin::BlockParam` onto the SAME variant,
/// `Param` — both are a binding the callee received from outside and
/// never refined, which is exactly what call-site parameter inference
/// (bead ita-djm) would close. `CallStats`'s `ret_*` fields count these
/// 1:1, plus `ret_unresolved` for the no-callee (`Inconclusive`/
/// `NotFound`) case, which is not a `RetCause` at all — see
/// `Checker::tally_unknown_receiver`.
#[derive(Copy, Clone, PartialEq, Eq)]
enum RetCause {
    /// Callee's return died on its own method/block parameter.
    Param,
    /// Callee's return died on an ivar that collapsed to Unknown.
    Ivar,
    /// Callee's return died on an unresolved constant.
    Const,
    /// Folded return Unknown via an explicit `return`; the tail
    /// expression itself was NOT Unknown.
    Explicit,
    /// Any other / unclassifiable cause — see `Checker::check_method_body`'s
    /// ceiling comment for the one case that actually lands here.
    Other,
}

fn ret_cause_of(o: UnkOrigin) -> RetCause {
    match o {
        UnkOrigin::Param | UnkOrigin::BlockParam => RetCause::Param,
        UnkOrigin::Ivar => RetCause::Ivar,
        UnkOrigin::Const => RetCause::Const,
        UnkOrigin::CoreRet | UnkOrigin::ProjectRet | UnkOrigin::Chain | UnkOrigin::Other => {
            RetCause::Other
        }
    }
}

/// Same walk as `check_file`, diagnostics discarded, call-site census
/// kept. ponytail: re-walks the file instead of widening `check_file`'s
/// tracked return type — only `ita check --stats` pays it.
pub fn call_stats(db: &dyn salsa::Database, file: SourceFile) -> CallStats {
    let text = file.text(db);
    let parse = ruby_prism::parse(text.as_bytes());
    let cast_comments = collect_cast_comments(
        text,
        parse.comments().map(|c| {
            let loc = c.location();
            (loc.start_offset(), loc.end_offset())
        }),
    );
    let index = project_index(db);
    let rbi_map = crate::RbiProject::try_get(db).map(|r| r.map(db));
    let mut checker = Checker {
        db,
        index,
        rbi_map,
        cast_comments,
        diags: Vec::new(),
        silent: false,
        dark: false,
        dark_recs: Vec::new(),
        returns: Vec::new(),
        return_memo: FxHashMap::default(),
        contract_memo: std::cell::RefCell::default(),
        return_cause_memo: FxHashMap::default(),
        in_progress: FxHashSet::default(),
        depth: 0,
        goto_target: None,
        goto_found: None,
        hover_target: None,
        hover_ty: None,
        hover_method: None,
        current_method_name: None,
        ivar_capture: None,
        ivar_class_memo: FxHashMap::default(),
        ivar_class_in_progress: FxHashSet::default(),
        stats: CallStats::default(),
        constraints: HashMap::new(),
        collect_constraint_outcomes: false,
        constraint_outcomes: Vec::new(),
        census: true,
        method_params: Vec::new(),
        block_params: Vec::new(),
        rebindable_block_depth: 0,
        local_origin: FxHashMap::default(),
        unknown_call_src: FxHashMap::default(),
        last_ret_cause: None,
        defined_guards: Vec::new(),
        respond_to_guards: Vec::new(),
        narrowed_names: Vec::new(),
        asserted_subject_spans: Vec::new(),
        operand_locals: FxHashMap::default(),
    };
    let mut env = Env::default();
    // Toplevel statements are one Ruby local scope; E0108's literal
    // proof is per scope (see `Checker::operand_locals`).
    checker.operand_locals = checker.scope_operand_locals(None, Some(&parse.node()));
    checker.walk_scope(&[], None, false, &parse.node(), &mut env);
    checker.stats
}

/// Go-to-definition: what does the call site at `offset` (a byte offset
/// into `file`'s current text) resolve to? Reuses the exact resolution
/// `check_call` already performs for E0101/arity/sig checks — same
/// `MethodLookup`, just with diagnostics suppressed (`silent: true`) and a
/// target offset to match against each call's message span. `None` covers
/// "no call at this offset" and `Inconclusive`/`NotFound` alike: never
/// guess the "best candidate".
pub fn definition_at(db: &dyn salsa::Database, file: SourceFile, offset: usize) -> Option<DefSite> {
    let text = file.text(db);
    let parse = ruby_prism::parse(text.as_bytes());
    let cast_comments = collect_cast_comments(
        text,
        parse.comments().map(|c| {
            let loc = c.location();
            (loc.start_offset(), loc.end_offset())
        }),
    );
    let index = project_index(db);
    let rbi_map = crate::RbiProject::try_get(db).map(|r| r.map(db));
    let mut checker = Checker {
        db,
        index,
        rbi_map,
        cast_comments,
        diags: Vec::new(),
        silent: true,
        dark: false,
        dark_recs: Vec::new(),
        returns: Vec::new(),
        return_memo: FxHashMap::default(),
        contract_memo: std::cell::RefCell::default(),
        return_cause_memo: FxHashMap::default(),
        in_progress: FxHashSet::default(),
        depth: 0,
        goto_target: Some(offset),
        goto_found: None,
        hover_target: None,
        hover_ty: None,
        hover_method: None,
        current_method_name: None,
        ivar_capture: None,
        ivar_class_memo: FxHashMap::default(),
        ivar_class_in_progress: FxHashSet::default(),
        stats: CallStats::default(),
        constraints: HashMap::new(),
        collect_constraint_outcomes: false,
        constraint_outcomes: Vec::new(),
        census: false,
        method_params: Vec::new(),
        block_params: Vec::new(),
        rebindable_block_depth: 0,
        local_origin: FxHashMap::default(),
        unknown_call_src: FxHashMap::default(),
        last_ret_cause: None,
        defined_guards: Vec::new(),
        respond_to_guards: Vec::new(),
        narrowed_names: Vec::new(),
        asserted_subject_spans: Vec::new(),
        operand_locals: FxHashMap::default(),
    };
    let mut env = Env::default();
    // Toplevel statements are one Ruby local scope; E0108's literal
    // proof is per scope (see `Checker::operand_locals`).
    checker.operand_locals = checker.scope_operand_locals(None, Some(&parse.node()));
    checker.walk_scope(&[], None, false, &parse.node(), &mut env);
    checker.goto_found
}

/// One resolved method call under the hover cursor: the signature pieces
/// hover is allowed to show. `arity` is a short summary (`"2"`, `"1..2"`
/// with optional params, `"0+"` with rest, `"unknown"` when the def's
/// arity can't be known); `site` is the same name-span `ita definition`
/// navigates to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MethodHover {
    /// The resolved method's own name — `initialize` under a `Foo.new`
    /// cursor, since that is the def the call runs.
    pub name: String,
    pub arity: String,
    pub site: DefSite,
}

/// Everything hover knows about one position. Both fields `None` means no
/// information: the caller answers nothing (LSP `null`), never a guess —
/// hover is under the same invariant #1 as diagnostics.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HoverInfo {
    /// Short rendered type of the innermost expression covering the
    /// position (`String`, `Foo`, `Foo | nil`); `None` when it inferred
    /// `Ty::Unknown`.
    pub ty: Option<String>,
    /// Signature of the resolved method call whose name the cursor sits
    /// on, if any.
    pub method: Option<MethodHover>,
}

/// Hover: what does the expression at `offset` (a byte offset into
/// `file`'s current text) infer to, and — when the offset sits on a
/// resolved call's method name — that call's signature? Same silent
/// `Checker` walk `definition_at` runs: diagnostics discarded, nothing
/// resolved twice, nothing new inferred — the types `check_file` already
/// computes are simply captured at the cursor instead of thrown away.
/// `None` when the position has no type information worth showing.
pub fn hover_at(db: &dyn salsa::Database, file: SourceFile, offset: usize) -> Option<HoverInfo> {
    let text = file.text(db);
    let parse = ruby_prism::parse(text.as_bytes());
    let cast_comments = collect_cast_comments(
        text,
        parse.comments().map(|c| {
            let loc = c.location();
            (loc.start_offset(), loc.end_offset())
        }),
    );
    let index = project_index(db);
    let rbi_map = crate::RbiProject::try_get(db).map(|r| r.map(db));
    let mut checker = Checker {
        db,
        index,
        rbi_map,
        cast_comments,
        diags: Vec::new(),
        silent: true,
        dark: false,
        dark_recs: Vec::new(),
        returns: Vec::new(),
        return_memo: FxHashMap::default(),
        contract_memo: std::cell::RefCell::default(),
        return_cause_memo: FxHashMap::default(),
        in_progress: FxHashSet::default(),
        depth: 0,
        goto_target: None,
        goto_found: None,
        hover_target: Some(offset),
        hover_ty: None,
        hover_method: None,
        current_method_name: None,
        ivar_capture: None,
        ivar_class_memo: FxHashMap::default(),
        ivar_class_in_progress: FxHashSet::default(),
        stats: CallStats::default(),
        constraints: HashMap::new(),
        collect_constraint_outcomes: false,
        constraint_outcomes: Vec::new(),
        census: false,
        method_params: Vec::new(),
        block_params: Vec::new(),
        rebindable_block_depth: 0,
        local_origin: FxHashMap::default(),
        unknown_call_src: FxHashMap::default(),
        last_ret_cause: None,
        defined_guards: Vec::new(),
        respond_to_guards: Vec::new(),
        narrowed_names: Vec::new(),
        asserted_subject_spans: Vec::new(),
        operand_locals: FxHashMap::default(),
    };
    let mut env = Env::default();
    // Toplevel statements are one Ruby local scope; E0108's literal
    // proof is per scope (see `Checker::operand_locals`).
    checker.operand_locals = checker.scope_operand_locals(None, Some(&parse.node()));
    checker.walk_scope(&[], None, false, &parse.node(), &mut env);
    let ty = checker
        .hover_ty
        .filter(|(_, _, t)| *t != Ty::Unknown)
        .map(|(_, _, t)| ty_name(&t, index));
    match (ty, checker.hover_method) {
        (None, None) => None,
        (ty, method) => Some(HoverInfo { ty, method }),
    }
}

/// Constraint outcomes for one file (bead ita-dqo, deliverable 2 payload
/// source): re-walk pattern, `call_stats`'s exact shape — only `ita check
/// --format=agent` pays the extra pass. Classifies every binding that
/// accumulated >= 2 distinct method constraints in a method scope
/// (`flush_constraints`'s dedup rule): `Contradiction` for the exact
/// cases `check_file` would warn E0107 on, `Inferred`/`UnionCandidate`
/// for the cases that stay silent because a real type (or a small set of
/// them) actually satisfies every constraint.
pub fn constraint_report(db: &dyn salsa::Database, file: SourceFile) -> Vec<ConstraintOutcome> {
    let text = file.text(db);
    let parse = ruby_prism::parse(text.as_bytes());
    let cast_comments = collect_cast_comments(
        text,
        parse.comments().map(|c| {
            let loc = c.location();
            (loc.start_offset(), loc.end_offset())
        }),
    );
    let index = project_index(db);
    let rbi_map = crate::RbiProject::try_get(db).map(|r| r.map(db));
    let mut checker = Checker {
        db,
        index,
        rbi_map,
        cast_comments,
        diags: Vec::new(),
        silent: false,
        dark: false,
        dark_recs: Vec::new(),
        returns: Vec::new(),
        return_memo: FxHashMap::default(),
        contract_memo: std::cell::RefCell::default(),
        return_cause_memo: FxHashMap::default(),
        in_progress: FxHashSet::default(),
        depth: 0,
        goto_target: None,
        goto_found: None,
        hover_target: None,
        hover_ty: None,
        hover_method: None,
        current_method_name: None,
        ivar_capture: None,
        ivar_class_memo: FxHashMap::default(),
        ivar_class_in_progress: FxHashSet::default(),
        stats: CallStats::default(),
        constraints: HashMap::new(),
        collect_constraint_outcomes: true,
        constraint_outcomes: Vec::new(),
        census: false,
        method_params: Vec::new(),
        block_params: Vec::new(),
        rebindable_block_depth: 0,
        local_origin: FxHashMap::default(),
        unknown_call_src: FxHashMap::default(),
        last_ret_cause: None,
        defined_guards: Vec::new(),
        respond_to_guards: Vec::new(),
        narrowed_names: Vec::new(),
        asserted_subject_spans: Vec::new(),
        operand_locals: FxHashMap::default(),
    };
    let mut env = Env::default();
    // Toplevel statements are one Ruby local scope; E0108's literal
    // proof is per scope (see `Checker::operand_locals`).
    checker.operand_locals = checker.scope_operand_locals(None, Some(&parse.node()));
    checker.walk_scope(&[], None, false, &parse.node(), &mut env);
    checker.constraint_outcomes
}

/// One binding's classified constraint outcome (bead ita-dqo). Only
/// `constraint_report` produces these — `check_file`'s own walk only
/// ever turns the `Contradiction` shape into an E0107 diagnostic, never
/// exposes `Inferred`/`UnionCandidate` (those never fire a diagnostic;
/// invariant #1 gives false negatives, not manufactured precision).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConstraintOutcome {
    /// Exactly one candidate satisfies every constraint.
    Inferred {
        receiver: String,
        ty: String,
        calls: Vec<ConstraintCall>,
    },
    /// 2..=4 candidates satisfy every constraint.
    UnionCandidate {
        receiver: String,
        candidates: Vec<String>,
        calls: Vec<ConstraintCall>,
    },
    /// Empty intersection (same payload as the E0107 the check walk emits).
    Contradiction(ConstraintProof),
}

/// The 8 concrete core receiver classes constraint candidates are drawn
/// from (bead ita-dqo) — `CoreClass::Object`/`Kernel` excluded
/// deliberately; see `Checker::constraint_candidates`'s doc comment.
const CONSTRAINT_CORE_CLASSES: &[CoreClass] = &[
    CoreClass::Integer,
    CoreClass::Float,
    CoreClass::Str,
    CoreClass::Sym,
    CoreClass::Array,
    CoreClass::Hash,
    CoreClass::Nil,
    CoreClass::Bool,
];

#[derive(Copy, Clone)]
enum SelfTy {
    Instance(ClassId),
    Class(ClassId),
    Unknown,
}

impl SelfTy {
    fn as_ty(self) -> Ty {
        match self {
            SelfTy::Instance(c) => Ty::Instance(c),
            SelfTy::Class(c) => Ty::Class(c),
            SelfTy::Unknown => Ty::Unknown,
        }
    }
}

/// A flow-narrowing fact extracted from an `is_a?`/`nil?` predicate on a
/// local variable or parameter (bead ita-u1t). Ivar narrowing is out of
/// scope by contract: a method call can mutate an ivar at any point, so
/// only the ivar's overall type is inferred (`ivar_ty`), never refined by
/// flow.
struct Narrow {
    var: String,
    kind: NarrowKind,
}

enum NarrowKind {
    /// `x.is_a?(Foo)`: true branch narrows to `Instance(Foo)`; false
    /// branch has no new information in general (contract: "no else
    /// info") EXCEPT when the receiver's current type is a closed union
    /// (bead ita-zdy) — see `eliminate_union_member`, which
    /// `apply_narrow_false` calls for this arm.
    IsA(Ty),
    /// `x.nil?`: true branch narrows to `Nil`; false branch strips `Nil`
    /// out of whatever was already known.
    NilCheck,
    /// Bare truthy predicate on a local var/param — `if x`, `unless x`,
    /// ternary `x ? a : b` — with no `.nil?`/`.is_a?` call (bead
    /// ita-w9i, family a: guards `narrow_of` previously couldn't see).
    /// True branch (`x` is truthy) strips `Nil`, same math as
    /// `NilCheck`'s false branch. False branch is deliberately a no-op:
    /// Ruby's falsy set is `nil | false`, and we cannot tell which one
    /// held, so narrowing to `Nil` there could manufacture a diagnostic
    /// on an actual `false` value (invariant #1).
    Truthy,
    /// `x.nil? || <anything>` (or the mirror `<anything> || x.nil?`) as
    /// a whole `if`/`unless` predicate (bead ita-w9i, family a). True
    /// branch is a no-op — the `||` can hold for a reason unrelated to
    /// `x` being nil, so narrowing there would risk a false positive.
    /// False branch strips `Nil`: the *only* way `a.nil? || b` is false
    /// is `a.nil?` itself being false.
    NilCheckOrGuard,
}

type Contract = (crate::sorbet_sig::SorbetSig, Vec<String>);
type ContractKey = (SourceFile, (usize, usize), String);

/// Work the Sorbet contract path did on this thread, counted so that a
/// regression test can bound it without a wall clock.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ContractWork {
    /// Family members `ProjectIndex::contract_dispatch_diverges` visited.
    pub family_visits: u64,
    /// Uncached `effective_sorbet` answers.
    pub contract_resolutions: u64,
    /// `sig_fill` calls on the return path.
    pub return_probes: u64,
}

thread_local! {
    static CONTRACT_WORK: std::cell::Cell<ContractWork> = std::cell::Cell::new(ContractWork::default());
}

/// This thread's contract work so far.
#[doc(hidden)]
#[must_use]
pub fn contract_work() -> ContractWork {
    CONTRACT_WORK.with(std::cell::Cell::get)
}

pub(crate) fn note_contract_work(update: impl FnOnce(&mut ContractWork)) {
    CONTRACT_WORK.with(|cell| {
        let mut work = cell.get();
        update(&mut work);
        cell.set(work);
    });
}

struct Checker<'db> {
    db: &'db dyn salsa::Database,
    index: &'db ProjectIndex,
    /// `RbiProject`'s phase-1 `constant name -> EVERY declaring .rbi
    /// file` map (bead ita-vto, unioned by bead ita-k9j.3). `None` when
    /// no client `sorbet/rbi` was discovered — every call site using it
    /// degrades to today's exact behavior. `check_const_ref` consults
    /// `index::rbi_declares` for a hit, never mutating `index` itself —
    /// see that function's doc comment for why.
    rbi_map: Option<&'db std::collections::HashMap<String, Vec<std::path::PathBuf>>>,
    diags: Vec<Diagnostic>,
    /// Foreign-body return inference runs silent: no diagnostics emitted.
    silent: bool,
    /// Dark singleton census: set only by `check_file_inner` when called
    /// through `check_file_dark`. Records the `Ty::Class` arms' verdicts,
    /// never a diagnostic.
    dark: bool,
    dark_recs: Vec<DarkSingleton>,
    /// Explicit `return` types of the method body currently being inferred.
    returns: Vec<Ty>,
    return_memo: FxHashMap<(ClassId, String, bool), Ty>,
    /// `effective_sorbet` answers by definition and called name.
    contract_memo: std::cell::RefCell<FxHashMap<ContractKey, Option<Contract>>>,
    /// Bead ita-mv5: same `(ClassId, name, singleton)` key as
    /// `return_memo`, written/read alongside it — only ever holds an
    /// entry when the memoized return WAS `Ty::Unknown` (a known return
    /// has no cause to remember).
    return_cause_memo: FxHashMap<(ClassId, String, bool), RetCause>,
    in_progress: FxHashSet<(ClassId, String, bool)>,
    /// Recursion depth guard for pathological nesting.
    depth: u32,
    /// Go-to-definition mode: byte offset to match call sites against.
    goto_target: Option<usize>,
    /// The first (only, by construction — offsets pick at most one call)
    /// match found while walking in go-to-definition mode.
    goto_found: Option<DefSite>,
    /// Hover mode (w4): byte offset the cursor sits at. Cleared around the
    /// nested silent walks (`method_return`, `ivar_ty`) — those re-walk
    /// OTHER methods' bodies, whose byte offsets belong to a different
    /// file, so capturing there could answer about a span that isn't the
    /// cursor's own file. `None` outside hover mode.
    hover_target: Option<usize>,
    /// Hover capture, expression side: the innermost expression whose span
    /// covers `hover_target` and its inferred type (`Unknown` kept only
    /// until something better or the walk ends — `hover_at` drops it).
    hover_ty: Option<(usize, usize, Ty)>,
    /// Hover capture, call side: the resolved method call whose message
    /// span covers `hover_target`.
    hover_method: Option<MethodHover>,
    /// Name of the method whose body is currently being walked, so `super`
    /// (bead ita-53y) knows which method it's continuing the lookup for.
    /// `None` outside any method body (class-body statements, toplevel).
    current_method_name: Option<String>,
    /// Bead ita-9p9 (perf): while `Some((class, _))`, every `@name = expr`
    /// write on `class` pushes its RHS type into the per-name bucket — the
    /// side channel `ivar_ty` uses while silently re-walking the class's
    /// methods exactly once to collect every ivar at once.
    ivar_capture: Option<(ClassId, HashMap<String, Vec<Ty>>)>,
    ivar_class_memo: FxHashMap<ClassId, HashMap<String, Ty>>,
    ivar_class_in_progress: FxHashSet<ClassId>,
    /// Call-site coverage census, filled only on the outer (non-`silent`)
    /// walk so nested return/ivar inference never double-counts.
    stats: CallStats,
    /// Per-method accumulator for bead ita-dqo's usage-constraint
    /// collection: binding name -> `(method, call start, call end)` in
    /// program order, not yet resolved to candidates (deferred to
    /// `flush_constraints`, which only pays for bindings that end up with
    /// >= 2 distinct methods). Reset (`take`/restore) around every
    /// > `check_method_body` call, exactly like `returns` and
    /// > `current_method_name`, so a nested silent re-walk of a foreign
    /// > method (`method_return`/`ivar_ty`) can never mix its own
    /// > constraints into the method actually being checked — reinforced
    /// > by `collect_constraint`'s own `!self.silent` gate, since nested
    /// > walks always run silent.
    constraints: HashMap<String, Vec<(String, usize, usize)>>,
    /// `constraint_report` mode: classify every qualifying binding into
    /// `constraint_outcomes` instead of only ever emitting E0107.
    /// `false` in every other entry point (byte-identical E0107-only
    /// behavior).
    collect_constraint_outcomes: bool,
    constraint_outcomes: Vec<ConstraintOutcome>,
    /// Bead ita-au5: `true` only in `call_stats`'s own `Checker` literal —
    /// every other entry point pays a single `bool` check for this whole
    /// sub-bucket census (cost discipline: `ita check` without `--stats`
    /// gets none of the work below).
    census: bool,
    /// This method's own parameter names (requireds/optionals/rest/
    /// `keywords/keyword_rest/the` `&block` param) — bead ita-au5's `Param`
    /// origin. Take/restore around `check_method_body` exactly like
    /// `constraints`, so a nested silent re-walk of a different method
    /// never leaks its params into the method actually being classified.
    method_params: Vec<String>,
    /// Stack of in-scope block/lambda parameter names — bead ita-au5's
    /// `BlockParam` origin. Pushed by `add_block_params`'s sink before a
    /// block/lambda body walk, truncated back after — nesting scope, not
    /// a full save/restore (blocks nest, they don't replace each other).
    block_params: Vec<String>,
    /// Depth of enclosing block/lambda bodies whose `self` could NOT be
    /// proven lexical. A self-send (receiverless call) inside such a block
    /// never produces a conclusive E0101: the method receiving the block
    /// may `instance_exec`/`instance_eval` it against another object —
    /// rebinding `self` while leaving locals untouched (which is also why
    /// only SELF-sends are softened; explicit-receiver calls in blocks
    /// stay fully checked). Found on the corpus: DSL classes rebinding
    /// field blocks against a context object; the sig carve-out (bead
    /// ita-4xy) closed those classes and exposed the gap.
    /// Saved/zeroed/restored around `check_method_body` like
    /// `method_params` — a nested silent re-walk of a callee's body is not
    /// inside the caller's block.
    ///
    /// Bead ita-uye: a block whose receiving call IS proven lexical
    /// (`block_keeps_lexical_self`) does not increment this, so
    /// `[1, 2].each { typo }` is conclusive again. Nesting composes for
    /// free: an `each` inside a rebound DSL block leaves the count at 1,
    /// because the enclosing block already made `self` unknowable.
    rebindable_block_depth: usize,
    /// Last-write-wins local-variable origin (bead ita-au5, ponytail:
    /// flow-insensitive — see the `LocalVariableWriteNode` arm's comment
    /// for the upgrade path): name -> the `unknown_origin` classification
    /// of the most recent `x = <expr>` whose RHS inferred `Unknown`/
    /// `Union`. Removed on a write that resolves to a known type.
    local_origin: FxHashMap<String, UnkOrigin>,
    /// Bead ita-au5: span (`span_of_call`) -> why THAT call site itself
    /// resolved to `Ty::Unknown` (the `UnkOrigin`), plus — bead ita-mv5,
    /// `ProjectRet` sites only — the `RetCause` naming WHY the callee's
    /// own return died on Unknown, so an outer call chained off it can
    /// attribute its own `unknown_receiver` hit to the real upstream
    /// cause instead of generically "the receiver was a call". Written
    /// and read only on the primary (non-`silent`) walk — see
    /// `note_unknown_origin`.
    unknown_call_src: FxHashMap<(usize, usize), (UnkOrigin, Option<RetCause>)>,
    /// Bead ita-mv5: the `RetCause` `check_method_body` just classified
    /// for the method body it finished walking (`census`-gated, set on
    /// EVERY `check_method_body` call, silent or not). `method_return`
    /// reads this immediately after its own `check_method_body` call
    /// (the freshly-computed path) to pair it with `return_cause_memo`;
    /// no other consumer may rely on it surviving past that one read —
    /// the next `check_method_body` call anywhere overwrites it.
    last_ret_cause: Option<RetCause>,
    /// Bead ita-qst: `#: as <target>` inline-cast comments for whichever
    /// file's AST is CURRENTLY being walked, keyed by physical-line byte
    /// range — see `collect_cast_comments`. Recomputed (never reused
    /// as-is) by every nested cross-file re-walk (`method_return`,
    /// `walk_class_ivars`) around its own `check_method_body` call, the
    /// same save/restore discipline `hover_target` already follows,
    /// because a byte offset is only meaningful against the file it came
    /// from.
    cast_comments: Vec<((usize, usize), CastTarget)>,
    /// Bead ita-r8k: constant paths currently proven to exist by an
    /// enclosing `defined?(X)` guard — pushed before walking the branch
    /// that only runs when the guard held (`if`/ternary then-branch,
    /// `unless` else-clause; see `defined_guard_const`), popped right
    /// after. `check_const_ref` treats membership here as a suppression
    /// channel exactly like the RBI channels beside it: a hit only
    /// silences E0104, it never changes what `infer_const` types the
    /// reference as (invariant #1). A stack, not a single slot, so
    /// nested guards compose the same way nested branches do; the
    /// measured sites (bead ita-r8k) never nest, but there is no reason
    /// this shape should silently misbehave if a corpus ever does.
    defined_guards: Vec<String>,
    /// Bead ita-w2c: method names currently proven to exist by an
    /// enclosing `respond_to?(:m)` / `respond_to?(:m, true)` guard —
    /// pushed before walking the branch that only runs when the guard
    /// held (`if`/ternary then-branch, `unless` else-clause; see
    /// `respond_to_guard_name`), popped right after. Consulted only for
    /// a RECEIVERLESS call of that exact name: `respond_to?` proves what
    /// `self` answers, never what some other object does, so
    /// `x.respond_to?(:m); x.m` stays conclusive. Suppression-only, the
    /// same contract as `defined_guards` beside it (invariant #1).
    respond_to_guards: Vec<String>,
    /// Bead ita-w2c: names whose reads are `Ty::Unknown` for the
    /// sub-expression currently being walked, because a `&&` operand to
    /// the left proved something the checker cannot model — today only
    /// `x.is_a?(Class) | x.is_a?(Module)`, which proves `x` is a class or
    /// module OBJECT and so whose method table (the singleton one the
    /// call really dispatches on) is unknowable. Narrowing to Unknown
    /// (never to a concrete accusable type) is what keeps the whole
    /// mechanism fail-closed. Keyed by NAME — a local variable read or a
    /// receiverless, argument-less, block-less call — because the two
    /// occurrences of `x` in `x.is_a?(Class) && x < Base` are distinct
    /// call nodes with distinct spans. Pushed by the `AndNode` arm around
    /// its right operand only and popped right after; see
    /// `Checker::class_object_guard`.
    narrowed_names: Vec<(String, Ty)>,
    /// Bead ita-w2c: spans (`span_of_call`) of calls that are the DIRECT
    /// subject of an asserted raise — minitest's `assert_raises(...) { X }`
    /// block body, or the `expect { X }` block of `RSpec`'s
    /// `.to raise_error(...)`. A call at one of these spans raises on
    /// purpose: its exception (arity included) is the ASSERTED behavior,
    /// not a defect. Span-keyed, not depth-counted, because the owner's
    /// form is deliberately narrow: a call nested any deeper inside the
    /// block keeps firing, and a span identifies exactly the one call
    /// node that is the assertion's subject (see
    /// `Checker::asserted_raise_subjects`, which pushes these around the
    /// sub-walk that contains the block). Suppression-only.
    asserted_subject_spans: Vec<(usize, usize)>,
    /// E0108: the flow-INSENSITIVE literal proof for the local variables
    /// of the ONE Ruby scope currently being walked — a method body, a
    /// class/module body, or the toplevel. Rebuilt and restored by
    /// whoever opens that scope, the same save/restore discipline
    /// `method_params` already follows, because a local name means
    /// nothing outside its own scope. A name is present only when EVERY
    /// write to it in that scope is a literal of the same core type and
    /// nothing else can rebind it (see `prove_operand_locals`); empty on
    /// every `silent` walk, which emits nothing anyway.
    operand_locals: FxHashMap<String, Ty>,
}

impl Checker<'_> {
    fn tally(&mut self, b: Bucket) {
        if !self.census || self.silent {
            return;
        }
        let s = &mut self.stats;
        match b {
            Bucket::Resolved => s.resolved += 1,
            Bucket::Core => s.core += 1,
            Bucket::RbiMethod => s.rbi_method += 1,
            Bucket::DslMethod => s.dsl_method += 1,
            Bucket::ArApiMethod => s.ar_api_method += 1,
            Bucket::Inconclusive => s.inconclusive += 1,
            Bucket::UnknownReceiver => s.unknown_receiver += 1,
            Bucket::Diagnosed => s.diagnosed += 1,
        }
    }

    fn emit(
        &mut self,
        start: usize,
        end: usize,
        code: &'static str,
        sev: Severity,
        message: String,
    ) {
        self.emit_with(start, end, code, sev, message, None);
    }

    /// `emit` plus did-you-mean payload (w12 closure). The suggestion is
    /// computed by the caller *after* the diagnostic decision has already
    /// been made — attaching it can never change which diagnostics fire.
    fn emit_with(
        &mut self,
        start: usize,
        end: usize,
        code: &'static str,
        sev: Severity,
        message: String,
        suggestion: Option<Suggestion>,
    ) {
        if !self.silent {
            self.diags.push(Diagnostic {
                start,
                end,
                code,
                severity: sev,
                message,
                suggestion,
                constraint: None,
            });
        }
    }

    /// Rendering-only did-you-mean data for E0101 (w12 closure): closest
    /// name among exactly the surface the failed lookup already searched —
    /// this receiver's ancestry's instance methods, the set
    /// `lookup_method` scans — plus where the index defines it. Called
    /// only from the arm that has already decided to emit E0101, so it
    /// can never change a decision. `def` uses the same `MethodSig` file +
    /// name-span the go-to-definition infra (`maybe_goto`) resolves to.
    fn method_suggestion(&self, c: ClassId, name: &str) -> Option<Suggestion> {
        let (ancestors, _) = self.index.ancestors(c);
        let names = ancestors
            .iter()
            .flat_map(|&a| self.index.class(a).methods.keys().map(String::as_str));
        let best = closest_name(name, names)?;
        let def = match self.index.lookup_method(c, best) {
            MethodLookup::Found(m, _) => Some((m.file, m.name_span.0)),
            _ => None,
        };
        Some(Suggestion {
            name: best.to_string(),
            def,
        })
    }

    /// Rendering-only did-you-mean data for the class-object E0101: the
    /// closest name among exactly the surface the failed lookup searched
    /// — every ancestor's SINGLETON methods plus every `extend`ed
    /// module's instance methods. Same contract as
    /// `method_suggestion`: called only from the arm that has already
    /// decided to emit, so it can never change a decision, and it never
    /// reaches for a name the lookup itself would not have found.
    fn singleton_method_suggestion(&self, c: ClassId, name: &str) -> Option<Suggestion> {
        let (ancestors, _) = self.index.ancestors(c);
        let mut names: Vec<&str> = Vec::new();
        for &a in &ancestors {
            let class = self.index.class(a);
            names.extend(class.singleton_methods.keys().map(String::as_str));
            for ext in &class.extends {
                if let Some(mid) = self.index.resolve_const(&class.nesting, ext) {
                    names.extend(self.index.class(mid).methods.keys().map(String::as_str));
                }
            }
        }
        let best = closest_name(name, names.into_iter())?;
        let def = match self.index.lookup_singleton(c, best) {
            MethodLookup::Found(m, _) => Some((m.file, m.name_span.0)),
            _ => None,
        };
        Some(Suggestion {
            name: best.to_string(),
            def,
        })
    }

    /// Rendering-only did-you-mean data for E0104 (w12 closure): closest
    /// indexed constant within edit distance 2 — class/module paths from
    /// `by_path`, value constants from the new `index.consts` map. A bare
    /// typo compares against each candidate's last segment (`Usr` vs
    /// `User`); a qualified typo compares whole paths (`Foo::Barr` vs
    /// `Foo::Bar`). `def` (defined-at) attaches only when the winner is a
    /// value constant the index has a site for; class/module paths keep
    /// `None`, never invented.
    fn const_suggestion(&self, path: &str) -> Option<Suggestion> {
        // (display, key): candidates keep their lookup key so a bare-typo
        // win on `Foo::Bar`'s last segment can still find `Foo::Bar`'s
        // def site.
        fn display(bare: bool, key: &str) -> &str {
            if bare { key.rsplit("::").next().unwrap_or(key) } else { key }
        }
        let consts = crate::index::project_consts(self.db);
        let bare = !path.contains("::");
        let candidates = self
            .index
            .by_path
            .keys()
            .chain(consts.keys())
            .map(|key| (display(bare, key), key.as_str()));
        let key = closest_const_key(path, candidates)?;
        let def = consts.get(key).map(|(file, span)| (*file, span.0));
        Some(Suggestion {
            name: key.to_string(),
            def,
        })
    }

    /// Closed-world mode on (bead ita-2ve)? Every behavioral change this
    /// bead makes — core-class narrowing, `x.extend(M)` widening, the
    /// conclusive core lookup — is gated here, so with the input absent
    /// (`ita server`, LSP, corpora with gems, any library caller that
    /// didn't opt in) the checker is byte-identical to v0.
    fn closed_world(&self) -> bool {
        crate::ClosedWorld::try_get(self.db).is_some_and(|cw| *cw.enabled(self.db))
    }

    /// In go-to-definition mode, record `m`'s definition site when `target`
    /// falls within the call's message (method-name) span. In hover mode,
    /// the same span match captures the call's signature (`resolved` is
    /// the def's own name — `initialize` under a `new` cursor). A no-op in
    /// normal diagnostic mode (both targets `None`).
    fn maybe_goto(&mut self, msg_loc: (usize, usize), m: &MethodSig, resolved: &str) {
        if let Some(target) = self.goto_target {
            if target >= msg_loc.0 && target <= msg_loc.1 {
                self.goto_found = Some(DefSite {
                    file: m.file,
                    start: m.name_span.0,
                    end: m.name_span.1,
                });
            }
        }
        if let Some(target) = self.hover_target {
            if target >= msg_loc.0 && target <= msg_loc.1 {
                self.hover_method = Some(MethodHover {
                    name: resolved.to_string(),
                    arity: arity_str(m),
                    site: DefSite {
                        file: m.file,
                        start: m.name_span.0,
                        end: m.name_span.1,
                    },
                });
            }
        }
    }

    /// `super`/`super(...)` navigation (bead ita-53y): like `maybe_goto`,
    /// but the target isn't a plain call — it's whatever comes after the
    /// class/module that physically defines the method currently being
    /// walked. Only pays for `ProjectIndex::super_lookup`'s project-wide
    /// scan when `target` actually falls inside this `super`'s span, so
    /// the normal (non-navigation) diagnostic pass — which never sets
    /// `goto_target` — never runs it at all.
    fn maybe_goto_super(&mut self, loc: (usize, usize), self_ty: SelfTy) {
        let Some(target) = self.goto_target else {
            return;
        };
        if target < loc.0 || target > loc.1 {
            return;
        }
        let Some(name) = self.current_method_name.clone() else {
            return;
        };
        let (owner, singleton) = match self_ty {
            SelfTy::Instance(c) => (c, false),
            SelfTy::Class(c) => (c, true),
            SelfTy::Unknown => return,
        };
        if let MethodLookup::Found(m, _) = self.index.super_lookup(owner, singleton, &name) {
            self.goto_found = Some(DefSite {
                file: m.file,
                start: m.name_span.0,
                end: m.name_span.1,
            });
        }
    }

    // -- scope walking (mirrors index::DefWalker shapes) --------------------

    fn walk_scope(
        &mut self,
        scope: &[String],
        class: Option<ClassId>,
        in_singleton: bool,
        node: &Node<'_>,
        env: &mut Env,
    ) {
        if let Some(prog) = node.as_program_node() {
            self.walk_scope(
                scope,
                class,
                in_singleton,
                &prog.statements().as_node(),
                env,
            );
        } else if let Some(stmts) = node.as_statements_node() {
            for stmt in &stmts.body() {
                self.scope_stmt(scope, class, in_singleton, &stmt, env);
            }
        } else if let Some(begin) = node.as_begin_node() {
            if let Some(stmts) = begin.statements() {
                self.walk_scope(scope, class, in_singleton, &stmts.as_node(), env);
            }
        } else {
            self.scope_stmt(scope, class, in_singleton, node, env);
        }
    }

    fn scope_stmt(
        &mut self,
        scope: &[String],
        class: Option<ClassId>,
        in_singleton: bool,
        node: &Node<'_>,
        env: &mut Env,
    ) {
        match node {
            Node::ClassNode { .. } => {
                let c = node.as_class_node().unwrap();
                let Some(path) = const_path_str(&c.constant_path()) else {
                    return;
                };
                let cur = scope.last().map_or("", String::as_str);
                let full = join_path(cur, &path);
                // Bead ita-519: exactly ONE new `Module.nesting` entry per
                // `class`/`module` keyword, mirroring `index.rs`'s
                // `DefWalker` — never one entry per `::`-segment in
                // `full` (see `ProjectIndex::resolve_const`'s doc
                // comment).
                let mut child_scope = scope.to_vec();
                child_scope.push(full.clone());
                let id = self.index.by_path.get(&full).copied();
                if let (Some(sc), Some(sc_node)) = (
                    c.superclass().and_then(|s| const_path_str(&s)),
                    c.superclass(),
                ) {
                    self.check_const_ref(&child_scope, &sc, &sc_node);
                }
                if let Some(body) = c.body() {
                    let mut class_env = Env::default();
                    let proven = self.scope_operand_locals(None, Some(&body));
                    let saved_operands = std::mem::replace(&mut self.operand_locals, proven);
                    self.walk_scope(&child_scope, id, false, &body, &mut class_env);
                    self.operand_locals = saved_operands;
                }
            }
            Node::ModuleNode { .. } => {
                let m = node.as_module_node().unwrap();
                let Some(path) = const_path_str(&m.constant_path()) else {
                    return;
                };
                let cur = scope.last().map_or("", String::as_str);
                let full = join_path(cur, &path);
                let mut child_scope = scope.to_vec();
                child_scope.push(full.clone());
                let id = self.index.by_path.get(&full).copied();
                if let Some(body) = m.body() {
                    let mut class_env = Env::default();
                    let proven = self.scope_operand_locals(None, Some(&body));
                    let saved_operands = std::mem::replace(&mut self.operand_locals, proven);
                    self.walk_scope(&child_scope, id, false, &body, &mut class_env);
                    self.operand_locals = saved_operands;
                }
            }
            Node::SingletonClassNode { .. } => {
                let sc = node.as_singleton_class_node().unwrap();
                if sc.expression().as_self_node().is_some() {
                    if let Some(body) = sc.body() {
                        self.walk_scope(scope, class, true, &body, env);
                    }
                }
            }
            Node::DefNode { .. } => {
                let def = node.as_def_node().unwrap();
                let singleton =
                    in_singleton || def.receiver().is_some_and(|r| r.as_self_node().is_some());
                let sig = self.sig_of(class, &def, singleton);
                self.check_method_body(&def, scope, class, singleton, sig.as_ref());
            }
            _ => {
                // Plain statement in class body / toplevel.
                let self_ty = match (class, in_singleton) {
                    (Some(c), _) if !scope.is_empty() => SelfTy::Class(c),
                    _ => SelfTy::Unknown,
                };
                self.infer_expr(node, env, self_ty, scope);
            }
        }
    }

    fn sig_of(&self, class: Option<ClassId>, def: &DefNode<'_>, singleton: bool) -> Option<RbsSig> {
        let class = class?;
        let name = String::from_utf8_lossy(def.name().as_slice()).into_owned();
        let cd = self.index.class(class);
        let m = if singleton {
            cd.singleton_methods.get(&name)
        } else {
            cd.methods.get(&name)
        };
        m.and_then(|m| m.sig.clone())
    }

    /// Inline metadata wins, including an unsupported inline signature.
    /// An RBI is eligible only for this exact source definition's owner,
    /// dispatch track and Ruby parameter layout.
    fn effective_sorbet(
        &self,
        method: &MethodSig,
        name: &str,
    ) -> Option<(crate::sorbet_sig::SorbetSig, Vec<String>)> {
        if method.sig.is_some() || method.arity_unknown || method.abstract_stub {
            return None;
        }
        if method.sorbet_sig.is_none() && (method.sorbet_annotated || self.rbi_map.is_none()) {
            return None;
        }
        // Every call to the method asks, and the answer is fixed by the
        // definition while the index is.
        let key = (method.file, method.def_span, name.to_owned());
        if let Some(hit) = self.contract_memo.borrow().get(&key) {
            return hit.clone();
        }
        note_contract_work(|w| w.contract_resolutions += 1);
        let contract = self.resolve_contract(method, name);
        self.contract_memo.borrow_mut().insert(key, contract.clone());
        contract
    }

    /// `effective_sorbet`'s uncached half.
    fn resolve_contract(
        &self,
        method: &MethodSig,
        name: &str,
    ) -> Option<(crate::sorbet_sig::SorbetSig, Vec<String>)> {
        let path = method.nesting.last()?;
        let (owner, name, singleton) = self.dispatching_name(method, name, path)?;
        // Cheap first: most unsigned methods have no RBI declaration, and
        // the dispatch walk below visits a whole family.
        let contract = if method.sorbet_annotated {
            method.sorbet_sig.clone().map(|sig| (sig, method.nesting.clone()))
        } else {
            let declaration = crate::index::source_rbi_method(path, name, singleton, self.rbi_map?)?;
            if !declaration.matches_source(method) {
                return None;
            }
            declaration.definition.sorbet_sig.map(|sig| (sig, declaration.nesting))
        }?;
        let diverges = self.index.contract_dispatch_diverges(owner, name, singleton, method.file, method.def_span);
        (!diverges).then_some(contract)
    }

    /// The owner, name and dispatch track this exact definition answers
    /// on, or `None` when the owner is open: no signature written there is
    /// proven to govern the call that reaches it.
    fn dispatching_name<'n>(
        &self,
        method: &MethodSig,
        name: &'n str,
        path: &str,
    ) -> Option<(ClassId, &'n str, bool)> {
        let owner = *self.index.by_path.get(path)?;
        let class = self.index.class(owner);
        if class.open {
            return None;
        }
        let same = |m: &MethodSig| m.file == method.file && m.def_span == method.def_span;
        let name = if name == "new" && class.methods.get("initialize").is_some_and(same) { "initialize" } else { name };
        let singleton = class.singleton_methods.get(name).is_some_and(same);
        Some((owner, name, singleton))
    }

    fn sorbet_of_def(
        &self,
        class: Option<ClassId>,
        def: &DefNode<'_>,
        singleton: bool,
    ) -> Option<(crate::sorbet_sig::SorbetSig, Vec<String>)> {
        let cd = self.index.class(class?);
        let name = String::from_utf8_lossy(def.name().as_slice());
        let method = if singleton { cd.singleton_methods.get(name.as_ref()) } else { cd.methods.get(name.as_ref()) }?;
        let loc = def.location();
        if method.def_span != (loc.start_offset(), loc.end_offset()) {
            return None;
        }
        self.effective_sorbet(method, &name)
    }

    /// Check a method body; returns the inferred return type (last expression
    /// unioned with explicit returns).
    fn check_method_body(
        &mut self,
        def: &DefNode<'_>,
        scope: &[String],
        class: Option<ClassId>,
        singleton: bool,
        sig: Option<&RbsSig>,
    ) -> Ty {
        let mut env = Env::default();
        let sorbet = self.sorbet_of_def(class, def, singleton);
        let saved_method_params = std::mem::take(&mut self.method_params);
        let saved_block_depth = std::mem::replace(&mut self.rebindable_block_depth, 0);
        if let Some(params) = def.parameters() {
            let type_params: &[String] = sig.map_or(&[], |s| s.type_params.as_slice());
            let mut sig_pos = sig.map(|s| {
                s.params
                    .iter()
                    .filter_map(|p| match p {
                        RbsParam::Required(t) | RbsParam::Optional(t) => Some(t),
                        RbsParam::Keyword { .. } => None,
                    })
                    .collect::<Vec<_>>()
            });
            let mut pos_i = 0usize;
            for p in &params.requireds() {
                if let Some(rp) = p.as_required_parameter_node() {
                    let name = String::from_utf8_lossy(rp.name().as_slice()).into_owned();
                    let ty = sig_pos
                        .as_mut()
                        .and_then(|v| v.get(pos_i).copied())
                        .map_or(Ty::Unknown, |t| self.rbs_to_ty(t, class, scope, type_params));
                    if self.census {
                        self.method_params.push(name.clone());
                    }
                    env.insert(name, ty);
                }
                pos_i += 1;
            }
            for p in &params.optionals() {
                if let Some(op) = p.as_optional_parameter_node() {
                    let name = String::from_utf8_lossy(op.name().as_slice()).into_owned();
                    let ty = sig_pos
                        .as_mut()
                        .and_then(|v| v.get(pos_i).copied())
                        .map_or(Ty::Unknown, |t| self.rbs_to_ty(t, class, scope, type_params));
                    if self.census {
                        self.method_params.push(name.clone());
                    }
                    env.insert(name, ty);
                }
                pos_i += 1;
            }
            if let Some(rest) = params.rest() {
                if let Some(rp) = rest.as_rest_parameter_node() {
                    if let Some(n) = rp.name() {
                        let name = String::from_utf8_lossy(n.as_slice()).into_owned();
                        if self.census {
                            self.method_params.push(name.clone());
                        }
                        env.insert(name, Ty::Array(Box::new(Ty::Unknown)));
                    }
                }
            }
            for kw in &params.keywords() {
                let (kname, kw_ty) = if let Some(k) = kw.as_required_keyword_parameter_node() {
                    (
                        String::from_utf8_lossy(k.name().as_slice()).into_owned(),
                        None,
                    )
                } else if let Some(k) = kw.as_optional_keyword_parameter_node() {
                    // default value may contain calls worth checking
                    let self_ty = self_ty_of(class, singleton);
                    let t = self.infer_expr(&k.value(), &mut env.clone(), self_ty, scope);
                    (
                        String::from_utf8_lossy(k.name().as_slice()).into_owned(),
                        Some(t),
                    )
                } else {
                    continue;
                };
                let sig_ty = sig.and_then(|s| {
                    s.params.iter().find_map(|p| match p {
                        RbsParam::Keyword { name, ty, .. } if *name == kname => Some(ty),
                        _ => None,
                    })
                });
                let ty = sig_ty.map_or(kw_ty.unwrap_or(Ty::Unknown), |t| {
                    self.rbs_to_ty(t, class, scope, type_params)
                });
                if self.census {
                    self.method_params.push(kname.clone());
                }
                env.insert(kname, ty);
            }
            if let Some(krest) = params.keyword_rest() {
                if let Some(kr) = krest.as_keyword_rest_parameter_node() {
                    if let Some(n) = kr.name() {
                        let name = String::from_utf8_lossy(n.as_slice()).into_owned();
                        if self.census {
                            self.method_params.push(name.clone());
                        }
                        env.insert(name, Ty::Hash(Box::new(Ty::Sym), Box::new(Ty::Unknown)));
                    }
                }
            }
            if let Some(block) = params.block() {
                if let Some(n) = block.name() {
                    let name = String::from_utf8_lossy(n.as_slice()).into_owned();
                    if self.census {
                        self.method_params.push(name.clone());
                    }
                    env.insert(name, Ty::Unknown);
                }
            }
        }
        if let Some((contract, nesting)) = &sorbet {
            for (name, expr) in &contract.params {
                let ty = crate::sorbet_sig::resolve_sig_ty(expr, self.index, nesting);
                if ty != Ty::Unknown && env.contains_key(name) {
                    env.insert(name.clone(), ty);
                }
            }
        }

        let self_ty = self_ty_of(class, singleton);
        // E0108's per-scope literal proof. The parameter list is handed
        // in so its names are POISONED: a parameter's runtime value
        // comes from the caller, so a body that also writes
        // `price = 100` has proven nothing about `price`.
        let proven = self.scope_operand_locals(
            def.parameters().map(|p| p.as_node()).as_ref(),
            def.body().as_ref(),
        );
        let saved_operands = std::mem::replace(&mut self.operand_locals, proven);
        let saved_returns = std::mem::take(&mut self.returns);
        let saved_name = self
            .current_method_name
            .replace(String::from_utf8_lossy(def.name().as_slice()).into_owned());
        let saved_constraints = std::mem::take(&mut self.constraints);
        let last = match def.body() {
            Some(body) => self.infer_expr(&body, &mut env, self_ty, scope),
            None => Ty::Nil,
        };
        self.flush_constraints();
        self.constraints = saved_constraints;
        self.current_method_name = saved_name;
        let last_is_unknownish = matches!(last, Ty::Unknown | Ty::Union(_));
        let returns = std::mem::replace(&mut self.returns, saved_returns);
        if let Some((contract, nesting)) = &sorbet {
            self.check_sorbet_return(def, contract, nesting, &last, &returns);
        }
        let folded = returns.into_iter().fold(last, Ty::union);
        if self.census {
            // Bead ita-mv5: classify WHY this method's own return died on
            // Unknown, keyed to the SAME `check_method_body` call whether
            // it ran for real or inside `method_return`'s silent re-walk
            // (gated on `census` alone, not `!self.silent` — see
            // `last_ret_cause`'s doc comment). MUST run before
            // `self.method_params` is restored below — `unknown_origin`
            // needs THIS method's own params still live to recognize its
            // tail as a `Param`/numbered-`BlockParam` read.
            self.last_ret_cause = if folded == Ty::Unknown {
                Some(if last_is_unknownish {
                    // The tail expression itself died on Unknown: blame
                    // whatever it read from (a param, an ivar, ...), the
                    // exact shape `unknown_origin` already recognizes.
                    //
                    // ponytail: when the tail is a CALL CHAIN (a
                    // `CallNode`, not a param/ivar/const read),
                    // `unknown_origin`'s `CallNode` arm depends on
                    // `unknown_call_src` — and when the tail is a plain
                    // local read whose write traced through a block
                    // param, it depends on `local_origin` — both
                    // `!self.silent`-gated writes (`note_unknown_origin`,
                    // `tally_unknown_receiver`, the `LocalVariableWriteNode`
                    // arm). Neither map is reset between
                    // `check_method_body` calls, so a method whose def
                    // ALSO got a non-`silent` primary scan earlier in
                    // THIS SAME walk (bead ita-au5's per-def pass, every
                    // `def` in the censused file) already has correct
                    // entries for its own spans, which this silent
                    // re-walk's classification reads back untouched
                    // (`ret_param_via_block_param` in
                    // `tests/ret_cause.rs` exercises exactly this). The
                    // ceiling only bites when NEITHER map was ever
                    // primed for that span — a callee whose def lives in
                    // a DIFFERENT file than the one being censused (never
                    // primary-scanned at all) or is reached ONLY via a
                    // silent re-walk (recursion, `ivar_ty`): there the
                    // chain/local-origin reads come back empty and this
                    // degrades to `RetCause::Other` (`ret_cause.rs`'s
                    // `ret_other` test — same-file here, but its tail is
                    // a two-call chain `unknown_origin` classifies as
                    // `Chain`, which `ret_cause_of` also maps to `Other`,
                    // so the ceiling and the ordinary `Chain->Other` fold
                    // are observationally identical either way). The
                    // `Param` bucket — the one bead ita-djm actually
                    // needs — is UNAFFECTED: `method_params` is populated
                    // straight off `def.parameters()` on every walk,
                    // silent or not (gated on `census` alone). Upgrade
                    // path, if the ceiling ever needs closing: per-file
                    // origin state keyed by `SourceFile` instead of one
                    // flat map.
                    // Ruby prism only wraps a method body in a
                    // `StatementsNode` when it has >= 2 statements; a
                    // single-statement body (the common case — see
                    // `ret_param`'s fixture) is the bare expression node
                    // itself, which IS its own tail.
                    let tail = def.body().and_then(|b| match b.as_statements_node() {
                        Some(s) => s.body().iter().last(),
                        None => Some(b),
                    });
                    ret_cause_of(self.unknown_origin(tail.as_ref()))
                } else {
                    // Tail was known; an explicit `return <Unknown>`
                    // elsewhere in the body is what folded this Unknown.
                    RetCause::Explicit
                })
            } else {
                None
            };
        }
        self.method_params = saved_method_params;
        self.rebindable_block_depth = saved_block_depth;
        self.operand_locals = saved_operands;
        folded
    }

    fn check_sorbet_return(
        &mut self,
        def: &DefNode<'_>,
        contract: &crate::sorbet_sig::SorbetSig,
        nesting: &[String],
        last: &Ty,
        returns: &[Ty],
    ) {
        if self.silent || contract.void {
            return;
        }
        let (Some(expr), Some(body)) = (contract.ret.as_deref(), def.body()) else { return };
        let mut safety = ReturnContractSafety::default();
        safety.visit(&body);
        if safety.uncertain {
            return;
        }
        let expected = crate::sorbet_sig::resolve_sig_ty(expr, self.index, nesting);
        // A bare `nil` is proven only where it is WRITTEN: a `return` with
        // no value or a literal `nil`, or a literal `nil` as the tail.
        let tail_nil = match body.as_statements_node() {
            Some(statements) => statements.body().iter().last().is_some_and(|n| matches!(n, Node::NilNode { .. })),
            None => matches!(body, Node::NilNode { .. }),
        };
        let returns_nil = safety.returns_literal_nil;
        // Explicit returns make the existing inference fold Unknown. Inspect
        // their proven values independently; never treat that Unknown as nil.
        let actual = returns.iter().find(|ty| contract_accuses(ty, &expected, returns_nil, self.index))
            .or_else(|| {
                (!return_terminal(&body) && contract_accuses(last, &expected, tail_nil, self.index)).then_some(last)
            });
        if let Some(actual) = actual {
            let loc = def.name_loc();
            self.emit(loc.start_offset(), loc.end_offset(), E0109_RETURN_TYPE_MISMATCH,
                Severity::Error, format!("return of `{}` expects {}, got {}",
                    String::from_utf8_lossy(def.name().as_slice()),
                    ty_name(&expected, self.index), ty_name(actual, self.index)));
        }
    }

    // -- expression inference ----------------------------------------------

    fn infer_expr(&mut self, node: &Node<'_>, env: &mut Env, self_ty: SelfTy, scope: &[String]) -> Ty {
        if self.depth > 200 {
            return Ty::Unknown;
        }
        self.depth += 1;
        let ty = self.infer_expr_inner(node, env, self_ty, scope);
        self.depth -= 1;
        self.maybe_hover_ty(node, &ty);
        ty
    }

    /// Hover capture, expression side (w4): remembers the innermost
    /// expression whose span covers the cursor. Strictly smaller spans
    /// win, so the cursor's own sub-expression beats every enclosing
    /// statement; a known type beats an earlier `Unknown` even at a
    /// larger span, but a known type is never overwritten by one. The
    /// cursor's honest answer can still be `Unknown` (e.g. hovering an
    /// untyped parameter) — `hover_at` turns that into "no info", never a
    /// guess.
    fn maybe_hover_ty(&mut self, node: &Node<'_>, ty: &Ty) {
        let Some(target) = self.hover_target else {
            return;
        };
        let loc = node.location();
        let (start, end) = (loc.start_offset(), loc.end_offset());
        if target < start || target > end {
            return;
        }
        let known = *ty != Ty::Unknown;
        let smaller = self
            .hover_ty
            .as_ref()
            .is_none_or(|(os, oe, _)| end - start < *oe - *os);
        if (known && smaller) || self.hover_ty.is_none() {
            self.hover_ty = Some((start, end, ty.clone()));
        }
    }

    fn infer_stmts(
        &mut self,
        stmts: &ruby_prism::StatementsNode<'_>,
        env: &mut Env,
        self_ty: SelfTy,
        scope: &[String],
    ) -> Ty {
        let mut last = Ty::Nil;
        for stmt in &stmts.body() {
            last = self.infer_expr(&stmt, env, self_ty, scope);
        }
        last
    }

    fn infer_expr_inner(
        &mut self,
        node: &Node<'_>,
        env: &mut Env,
        self_ty: SelfTy,
        scope: &[String],
    ) -> Ty {
        match node {
            Node::IntegerNode { .. } => Ty::Int,
            Node::FloatNode { .. } | Node::RationalNode { .. } | Node::ImaginaryNode { .. } => {
                Ty::Float
            }
            Node::StringNode { .. } | Node::XStringNode { .. } => Ty::Str,
            Node::SymbolNode { .. } => Ty::Sym,
            Node::TrueNode { .. } | Node::FalseNode { .. } => Ty::Bool,
            Node::NilNode { .. } => Ty::Nil,
            Node::SelfNode { .. } => self_ty.as_ty(),
            Node::SourceFileNode { .. } | Node::SourceLineNode { .. } => Ty::Str,
            Node::SourceEncodingNode { .. } => Ty::Unknown,

            Node::InterpolatedStringNode { .. } => {
                let n = node.as_interpolated_string_node().unwrap();
                for part in &n.parts() {
                    self.infer_expr(&part, env, self_ty, scope);
                }
                Ty::Str
            }
            Node::InterpolatedSymbolNode { .. } => {
                let n = node.as_interpolated_symbol_node().unwrap();
                for part in &n.parts() {
                    self.infer_expr(&part, env, self_ty, scope);
                }
                Ty::Sym
            }
            Node::InterpolatedXStringNode { .. } => Ty::Str,
            Node::EmbeddedStatementsNode { .. } => {
                let n = node.as_embedded_statements_node().unwrap();
                match n.statements() {
                    Some(s) => self.infer_stmts(&s, env, self_ty, scope),
                    None => Ty::Nil,
                }
            }
            Node::EmbeddedVariableNode { .. } => Ty::Unknown,
            Node::RegularExpressionNode { .. }
            | Node::InterpolatedRegularExpressionNode { .. }
            | Node::MatchLastLineNode { .. }
            | Node::InterpolatedMatchLastLineNode { .. } => Ty::Unknown,

            Node::ArrayNode { .. } => {
                let n = node.as_array_node().unwrap();
                let mut elem: Option<Ty> = None;
                let mut dynamic = false;
                for e in &n.elements() {
                    if let Some(s) = e.as_splat_node() {
                        if let Some(expr) = s.expression() {
                            self.infer_expr(&expr, env, self_ty, scope);
                        }
                        dynamic = true;
                        continue;
                    }
                    let t = self.infer_expr(&e, env, self_ty, scope);
                    elem = Some(match elem {
                        Some(prev) => Ty::union(prev, t),
                        None => t,
                    });
                }
                if dynamic {
                    Ty::Array(Box::new(Ty::Unknown))
                } else {
                    Ty::Array(Box::new(elem.unwrap_or(Ty::Unknown)))
                }
            }
            Node::HashNode { .. } | Node::KeywordHashNode { .. } => {
                let elements: Vec<Node<'_>> = if let Some(h) = node.as_hash_node() {
                    h.elements().iter().collect()
                } else {
                    node.as_keyword_hash_node().unwrap().elements().iter().collect()
                };
                let mut kt: Option<Ty> = None;
                let mut vt: Option<Ty> = None;
                let mut dynamic = false;
                for e in &elements {
                    if let Some(assoc) = e.as_assoc_node() {
                        let k = self.infer_expr(&assoc.key(), env, self_ty, scope);
                        let v = self.infer_expr(&assoc.value(), env, self_ty, scope);
                        kt = Some(kt.map_or(k.clone(), |p| Ty::union(p, k)));
                        vt = Some(vt.map_or(v.clone(), |p| Ty::union(p, v)));
                    } else {
                        dynamic = true;
                        if let Some(sp) = e.as_assoc_splat_node() {
                            if let Some(v) = sp.value() {
                                self.infer_expr(&v, env, self_ty, scope);
                            }
                        }
                    }
                }
                if dynamic {
                    Ty::Hash(Box::new(Ty::Unknown), Box::new(Ty::Unknown))
                } else {
                    Ty::Hash(
                        Box::new(kt.unwrap_or(Ty::Unknown)),
                        Box::new(vt.unwrap_or(Ty::Unknown)),
                    )
                }
            }
            Node::RangeNode { .. } => {
                let n = node.as_range_node().unwrap();
                if let Some(l) = n.left() {
                    self.infer_expr(&l, env, self_ty, scope);
                }
                if let Some(r) = n.right() {
                    self.infer_expr(&r, env, self_ty, scope);
                }
                Ty::Unknown
            }

            // -- variables --
            Node::LocalVariableReadNode { .. } => {
                let n = node.as_local_variable_read_node().unwrap();
                let name = String::from_utf8_lossy(n.name().as_slice()).into_owned();
                env.get(&name).cloned().unwrap_or(Ty::Unknown)
            }
            Node::LocalVariableWriteNode { .. } => {
                let n = node.as_local_variable_write_node().unwrap();
                let ty = self.infer_expr(&n.value(), env, self_ty, scope);
                // Bead ita-j0z: `x = <expr> #: as untyped` (ruby-lsp's
                // `addon_test.rb:149` shape — the cast comment trails
                // the RHS's own line, not a call argument) reuses the
                // exact same `apply_cast_comment` an argument cast does
                // — see that function's doc comment for the per-target
                // contract (`Untyped` erases unconditionally, `NotNil`
                // strips, an unresolvable `Named` leaves `ty` untouched).
                let ty = self.apply_cast_comment(ty, n.value().location().start_offset());
                let name = String::from_utf8_lossy(n.name().as_slice()).into_owned();
                self.kill_constraint(&name);
                if self.census && !self.silent {
                    if matches!(ty, Ty::Unknown | Ty::Union(_)) {
                        let origin = self.unknown_origin(Some(&n.value()));
                        self.local_origin.insert(name.clone(), origin);
                    } else {
                        self.local_origin.remove(&name);
                    }
                }
                env.insert(name, ty.clone());
                ty
            }
            Node::LocalVariableOrWriteNode { .. } => {
                let n = node.as_local_variable_or_write_node().unwrap();
                let ty = self.infer_expr(&n.value(), env, self_ty, scope);
                let name = String::from_utf8_lossy(n.name().as_slice()).into_owned();
                self.kill_constraint(&name);
                let prev = env.get(&name).cloned().unwrap_or(Ty::Nil);
                let joined = Ty::union(prev, ty);
                env.insert(name, joined.clone());
                joined
            }
            Node::LocalVariableAndWriteNode { .. } => {
                let n = node.as_local_variable_and_write_node().unwrap();
                self.infer_expr(&n.value(), env, self_ty, scope);
                let name = String::from_utf8_lossy(n.name().as_slice()).into_owned();
                self.kill_constraint(&name);
                env.insert(name, Ty::Unknown);
                Ty::Unknown
            }
            Node::LocalVariableOperatorWriteNode { .. } => {
                let n = node.as_local_variable_operator_write_node().unwrap();
                self.infer_expr(&n.value(), env, self_ty, scope);
                let name = String::from_utf8_lossy(n.name().as_slice()).into_owned();
                self.kill_constraint(&name);
                env.insert(name, Ty::Unknown);
                Ty::Unknown
            }
            Node::ItLocalVariableReadNode { .. } => Ty::Unknown,

            // gvars/cvars/backrefs: still Unknown — no query analogous to
            // `ivar_ty` exists for those (out of scope for this bead).
            Node::ClassVariableReadNode { .. }
            | Node::GlobalVariableReadNode { .. }
            | Node::BackReferenceReadNode { .. }
            | Node::NumberedReferenceReadNode { .. } => Ty::Unknown,
            // Instance-variable type (bead ita-u1t): union of every
            // `@name = <expr>` assignment across the class's instance
            // methods — see `ivar_ty`. Only known when `self` is a plain
            // instance (`SelfTy::Instance`); a singleton-context read
            // (`def self.foo; @x; end`) is a different storage slot and
            // stays Unknown, same as before this bead.
            Node::InstanceVariableReadNode { .. } => {
                let Some(n) = node.as_instance_variable_read_node() else {
                    return Ty::Unknown;
                };
                match self_ty {
                    SelfTy::Instance(c) => {
                        let name = String::from_utf8_lossy(n.name().as_slice()).into_owned();
                        self.ivar_ty(c, &name)
                    }
                    _ => Ty::Unknown,
                }
            }
            Node::InstanceVariableWriteNode { .. } => {
                let n = node.as_instance_variable_write_node().unwrap();
                let ty = self.infer_expr(&n.value(), env, self_ty, scope);
                // Side channel for `ivar_ty`'s silent re-walk: capture this
                // assignment's type when it matches the (class, name) it's
                // currently collecting for.
                self.capture_ivar_write(self_ty, n.name().as_slice(), ty.clone());
                ty
            }
            Node::ClassVariableWriteNode { .. } => {
                let n = node.as_class_variable_write_node().unwrap();
                self.infer_expr(&n.value(), env, self_ty, scope)
            }
            Node::GlobalVariableWriteNode { .. } => {
                let n = node.as_global_variable_write_node().unwrap();
                self.infer_expr(&n.value(), env, self_ty, scope)
            }
            // `||=`/`&&=`/op-assign ivar writes are not typed here, but they
            // ARE writes: each one feeds `ivar_ty` an Unknown, so an ivar
            // whose only literal write is `@x = nil` never reads back as
            // exactly nil once `@x ||= compute` exists.
            Node::InstanceVariableOrWriteNode { .. } => {
                let n = node.as_instance_variable_or_write_node().unwrap();
                self.infer_expr(&n.value(), env, self_ty, scope);
                self.capture_ivar_write(self_ty, n.name().as_slice(), Ty::Unknown);
                Ty::Unknown
            }
            Node::InstanceVariableAndWriteNode { .. } => {
                let n = node.as_instance_variable_and_write_node().unwrap();
                self.infer_expr(&n.value(), env, self_ty, scope);
                self.capture_ivar_write(self_ty, n.name().as_slice(), Ty::Unknown);
                Ty::Unknown
            }
            Node::InstanceVariableOperatorWriteNode { .. } => {
                let n = node.as_instance_variable_operator_write_node().unwrap();
                self.infer_expr(&n.value(), env, self_ty, scope);
                self.capture_ivar_write(self_ty, n.name().as_slice(), Ty::Unknown);
                Ty::Unknown
            }

            // -- constants --
            Node::ConstantReadNode { .. } | Node::ConstantPathNode { .. } => {
                self.infer_const(node, scope)
            }
            Node::ConstantWriteNode { .. } => {
                let n = node.as_constant_write_node().unwrap();
                self.infer_expr(&n.value(), env, self_ty, scope)
            }

            // -- control flow --
            Node::IfNode { .. } => {
                let n = node.as_if_node().unwrap();
                let narrow = self.narrow_of(&n.predicate(), scope);
                self.infer_expr(&n.predicate(), env, self_ty, scope);
                let mut then_env = env.clone();
                if let Some(nw) = &narrow {
                    apply_narrow_true(&mut then_env, nw);
                }
                // Bead ita-r8k: `defined?(X) ? X : Y` — the then-branch
                // only runs when `defined?` proved `X` exists, so a read
                // of that SAME constant in here is safe even though the
                // project index can't resolve it. Scoped to exactly this
                // branch (see `defined_guard_const`'s doc comment for why
                // only the direct `defined?(X)` predicate shape counts).
                let defined_guard = defined_guard_const(&n.predicate());
                if let Some(path) = &defined_guard {
                    self.defined_guards.push(path.clone());
                }
                // Bead ita-w2c: `if respond_to?(:setup); setup; end` — the
                // then-branch only runs when `self` really answers
                // `setup`, which is exactly the proof the receiverless
                // call inside it needs. Same branch scope as the
                // `defined?` guard beside it, and symmetric for
                // `unless` below.
                let respond_to_guard = respond_to_guard_name(&n.predicate());
                if let Some(m) = &respond_to_guard {
                    self.respond_to_guards.push(m.clone());
                }
                let then_stmts = n.statements();
                let then_diverges = then_stmts.as_ref().is_some_and(stmts_diverge);
                let then_ty = match &then_stmts {
                    Some(s) => self.infer_stmts(s, &mut then_env, self_ty, scope),
                    None => Ty::Nil,
                };
                if defined_guard.is_some() {
                    self.defined_guards.pop();
                }
                if respond_to_guard.is_some() {
                    self.respond_to_guards.pop();
                }
                let mut else_env = env.clone();
                if let Some(nw) = &narrow {
                    apply_narrow_false(&mut else_env, nw, env);
                }
                let has_else = n.subsequent().is_some();
                let else_ty = match n.subsequent() {
                    Some(s) => self.infer_expr(&s, &mut else_env, self_ty, scope),
                    None => Ty::Nil,
                };
                if narrow.is_some() && then_diverges && !has_else {
                    // `return if x.nil?` / `raise ... if x.nil?` (bead
                    // ita-u1t): the statement after this `if` is only ever
                    // reached when the predicate was false, so its state
                    // is the negated-fact branch, not a join.
                    *env = else_env;
                } else {
                    join_envs(env, &[then_env, else_env]);
                }
                Ty::union(then_ty, else_ty)
            }
            Node::UnlessNode { .. } => {
                let n = node.as_unless_node().unwrap();
                let narrow = self.narrow_of(&n.predicate(), scope);
                self.infer_expr(&n.predicate(), env, self_ty, scope);
                // `unless` runs its body when the predicate is FALSE, so
                // the roles are swapped relative to `IfNode`.
                let mut then_env = env.clone();
                if let Some(nw) = &narrow {
                    apply_narrow_false(&mut then_env, nw, env);
                }
                let then_stmts = n.statements();
                let then_diverges = then_stmts.as_ref().is_some_and(stmts_diverge);
                let then_ty = match &then_stmts {
                    Some(s) => self.infer_stmts(s, &mut then_env, self_ty, scope),
                    None => Ty::Nil,
                };
                let mut else_env = env.clone();
                if let Some(nw) = &narrow {
                    apply_narrow_true(&mut else_env, nw);
                }
                let has_else = n.else_clause().is_some();
                // Bead ita-r8k: `unless defined?(X); Y; else; X; end` —
                // the else-clause only runs when the predicate is TRUE,
                // i.e. `defined?` proved `X` exists (the `unless` mirror
                // of `IfNode`'s then-branch guard above).
                let defined_guard = defined_guard_const(&n.predicate());
                if let Some(path) = &defined_guard {
                    self.defined_guards.push(path.clone());
                }
                // Bead ita-w2c, the `unless` mirror: `unless
                // respond_to?(:setup); ...; else; setup; end` — the
                // else-clause only runs when the predicate was TRUE.
                let respond_to_guard = respond_to_guard_name(&n.predicate());
                if let Some(m) = &respond_to_guard {
                    self.respond_to_guards.push(m.clone());
                }
                let else_ty = match n.else_clause() {
                    Some(e) => match e.statements() {
                        Some(s) => self.infer_stmts(&s, &mut else_env, self_ty, scope),
                        None => Ty::Nil,
                    },
                    None => Ty::Nil,
                };
                if defined_guard.is_some() {
                    self.defined_guards.pop();
                }
                if respond_to_guard.is_some() {
                    self.respond_to_guards.pop();
                }
                // `unless x.is_a?(Foo); return; end` is deliberately
                // EXCLUDED from the shortcut below (bead ita-w9i):
                // `IfNode`'s existing `is_a?` shortcut (bead ita-u1t) was
                // already exercised by the pinned corpora baselines, but
                // the `unless` mirror is new here, and turning it on for
                // `IsA` too was observed to surface an unrelated,
                // pre-existing generic-method-type-param bug
                // (`rbs_to_ty`'s `RbsTy::Simple("T")` fallback resolving
                // "T" as a real constant instead of `Unknown`) at a call
                // site the old, less-precise flow never reached. None of
                // this bead's 32 target sites need `IsA` here — only
                // `NilCheck`/`NilCheckOrGuard`/`Truthy` do — so the
                // shortcut stays scoped to exactly those kinds.
                let shortcut_kind = narrow
                    .as_ref()
                    .is_some_and(|nw| !matches!(nw.kind, NarrowKind::IsA(_)));
                if shortcut_kind && then_diverges && !has_else {
                    // `return unless x` / `raise ... unless x` (bead
                    // ita-w9i, mirroring `IfNode`'s ita-u1t shortcut):
                    // the statement after this `unless` is only ever
                    // reached when the predicate was TRUE, so its state
                    // is the affirmed-fact branch, not a join.
                    *env = else_env;
                } else {
                    join_envs(env, &[then_env, else_env]);
                }
                Ty::union(then_ty, else_ty)
            }
            Node::ElseNode { .. } => {
                let n = node.as_else_node().unwrap();
                match n.statements() {
                    Some(s) => self.infer_stmts(&s, env, self_ty, scope),
                    None => Ty::Nil,
                }
            }
            Node::CaseNode { .. } => {
                let n = node.as_case_node().unwrap();
                // `case event; when ConstNodeAdded; ...` (bead ita-w9i,
                // family b): `when` uses `Module#===`, so a `when
                // <Class>` clause narrows the subject the same way
                // `subject.is_a?(<Class>)` would — only when the subject
                // is a plain local var/param read.
                let subject_var = n
                    .predicate()
                    .as_ref()
                    .and_then(Node::as_local_variable_read_node)
                    .map(|l| String::from_utf8_lossy(l.name().as_slice()).into_owned());
                if let Some(p) = n.predicate() {
                    self.infer_expr(&p, env, self_ty, scope);
                }
                let mut result: Option<Ty> = None;
                let mut branch_envs = Vec::new();
                for w in &n.conditions() {
                    if let Some(when) = w.as_when_node() {
                        for c in &when.conditions() {
                            self.infer_expr(&c, env, self_ty, scope);
                        }
                        let mut be = env.clone();
                        if let Some(var) = &subject_var {
                            if let Some(ty) = self.narrow_of_when(&when, scope) {
                                be.insert(var.clone(), ty);
                            }
                        }
                        let t = match when.statements() {
                            Some(s) => self.infer_stmts(&s, &mut be, self_ty, scope),
                            None => Ty::Nil,
                        };
                        result = Some(result.map_or(t.clone(), |p| Ty::union(p, t)));
                        branch_envs.push(be);
                    }
                }
                let mut ee = env.clone();
                let et = match n.else_clause() {
                    Some(e) => match e.statements() {
                        Some(s) => self.infer_stmts(&s, &mut ee, self_ty, scope),
                        None => Ty::Nil,
                    },
                    None => Ty::Nil,
                };
                branch_envs.push(ee);
                join_envs(env, &branch_envs);
                Ty::union(result.unwrap_or(Ty::Nil), et)
            }
            Node::CaseMatchNode { .. } => {
                // Pattern matching: too dynamic for v0; widen everything.
                let n = node.as_case_match_node().unwrap();
                if let Some(p) = n.predicate() {
                    self.infer_expr(&p, env, self_ty, scope);
                }
                widen_env(env);
                Ty::Unknown
            }
            Node::WhileNode { .. } => {
                let n = node.as_while_node().unwrap();
                self.infer_expr(&n.predicate(), env, self_ty, scope);
                let mut body_env = env.clone();
                if let Some(s) = n.statements() {
                    self.infer_stmts(&s, &mut body_env, self_ty, scope);
                }
                merge_loop_env(env, &body_env);
                Ty::Nil
            }
            Node::UntilNode { .. } => {
                let n = node.as_until_node().unwrap();
                self.infer_expr(&n.predicate(), env, self_ty, scope);
                let mut body_env = env.clone();
                if let Some(s) = n.statements() {
                    self.infer_stmts(&s, &mut body_env, self_ty, scope);
                }
                merge_loop_env(env, &body_env);
                Ty::Nil
            }
            Node::ForNode { .. } => {
                let n = node.as_for_node().unwrap();
                self.infer_expr(&n.collection(), env, self_ty, scope);
                let mut body_env = env.clone();
                if let Some(s) = n.statements() {
                    self.infer_stmts(&s, &mut body_env, self_ty, scope);
                }
                merge_loop_env(env, &body_env);
                Ty::Unknown
            }
            Node::AndNode { .. } => {
                let n = node.as_and_node().unwrap();
                let l = self.infer_expr(&n.left(), env, self_ty, scope);
                // Bead ita-w2c: `x.is_a?(Class) && x < Base` — the right
                // operand is only ever reached when the LEFT one held, so
                // `x` there is a class/module OBJECT, not whatever the
                // left operand's own read inferred. No `Ty` models a
                // singleton (the method table such a call really
                // dispatches on), and narrowing to the class's own
                // instance type would be a lie the checker would then
                // accuse against — so the narrowed reading is
                // `Ty::Unknown`, the fail-closed choice. Scoped to the
                // right operand alone: the left operand was already
                // walked, and the fact dies with this `&&`.
                let guarded = self.class_object_guard(&n.left(), scope);
                if let Some(name) = &guarded {
                    self.narrowed_names.push((name.clone(), Ty::Unknown));
                }
                let r = self.infer_expr(&n.right(), env, self_ty, scope);
                if guarded.is_some() {
                    self.narrowed_names.pop();
                }
                Ty::union(l, r)
            }
            Node::OrNode { .. } => {
                let n = node.as_or_node().unwrap();
                let l = self.infer_expr(&n.left(), env, self_ty, scope);
                let r = self.infer_expr(&n.right(), env, self_ty, scope);
                Ty::union(l, r)
            }
            Node::BeginNode { .. } => {
                let n = node.as_begin_node().unwrap();
                let init = env.clone();
                let mut body_ty = match n.statements() {
                    Some(s) => self.infer_stmts(&s, env, self_ty, scope),
                    None => Ty::Nil,
                };
                let mut rescue_envs = Vec::new();
                let mut rescue = n.rescue_clause();
                while let Some(r) = rescue {
                    for e in &r.exceptions() {
                        self.infer_expr(&e, env, self_ty, scope);
                    }
                    let mut re = init.clone();
                    if let Some(refn) = r.reference() {
                        if let Some(t) = refn.as_local_variable_target_node() {
                            re.insert(
                                String::from_utf8_lossy(t.name().as_slice()).into_owned(),
                                Ty::Unknown,
                            );
                        }
                    }
                    let t = match r.statements() {
                        Some(s) => self.infer_stmts(&s, &mut re, self_ty, scope),
                        None => Ty::Nil,
                    };
                    body_ty = Ty::union(body_ty, t);
                    rescue_envs.push(re);
                    rescue = r.subsequent();
                }
                if let Some(e) = n.else_clause() {
                    if let Some(s) = e.statements() {
                        body_ty = self.infer_stmts(&s, env, self_ty, scope);
                    }
                }
                if !rescue_envs.is_empty() {
                    let mut branches = rescue_envs;
                    branches.push(env.clone());
                    join_envs(env, &branches);
                }
                if let Some(ens) = n.ensure_clause() {
                    if let Some(s) = ens.statements() {
                        self.infer_stmts(&s, env, self_ty, scope);
                    }
                }
                body_ty
            }
            Node::RescueModifierNode { .. } => {
                let n = node.as_rescue_modifier_node().unwrap();
                let l = self.infer_expr(&n.expression(), env, self_ty, scope);
                let r = self.infer_expr(&n.rescue_expression(), env, self_ty, scope);
                Ty::union(l, r)
            }
            Node::EnsureNode { .. } => Ty::Nil,
            Node::ReturnNode { .. } => {
                let n = node.as_return_node().unwrap();
                let ty = match n.arguments() {
                    Some(args) => {
                        // Collected eagerly on purpose: `infer_expr` is a
                        // walk with side effects — it records constraints and
                        // can emit diagnostics — so every argument must be
                        // visited even when only the first decides the type.
                        // A lazy iterator here would silently stop walking
                        // after the second argument of `return a, b, c` and
                        // drop whatever diagnostics lived in the rest.
                        let tys: Vec<Ty> = args
                            .arguments()
                            .iter()
                            .map(|a| self.infer_expr(&a, env, self_ty, scope))
                            .collect();
                        // Draining and matching the first two items, rather
                        // than switching on the length, leaves nothing to
                        // unwrap: no value is Nil, one value is that value,
                        // and `return a, b` is an Array whose element type
                        // this checker does not claim to know.
                        let mut drained = tys.into_iter();
                        match (drained.next(), drained.next()) {
                            (None, _) => Ty::Nil,
                            (Some(only), None) => only,
                            (Some(_), Some(_)) => Ty::Array(Box::new(Ty::Unknown)),
                        }
                    }
                    None => Ty::Nil,
                };
                self.returns.push(ty);
                Ty::Unknown
            }
            Node::BreakNode { .. }
            | Node::NextNode { .. }
            | Node::RedoNode { .. }
            | Node::RetryNode { .. } => Ty::Unknown,
            Node::ParenthesesNode { .. } => {
                let n = node.as_parentheses_node().unwrap();
                match n.body() {
                    Some(b) => self.infer_expr(&b, env, self_ty, scope),
                    None => Ty::Nil,
                }
            }
            Node::StatementsNode { .. } => {
                let n = node.as_statements_node().unwrap();
                self.infer_stmts(&n, env, self_ty, scope)
            }
            Node::DefinedNode { .. } => Ty::Union(vec![Ty::Str, Ty::Nil]),

            // -- calls --
            Node::CallNode { .. } => {
                let call = node.as_call_node().unwrap();
                self.check_call(&call, env, self_ty, scope)
            }
            Node::CallOrWriteNode { .. } => {
                let n = node.as_call_or_write_node().unwrap();
                if let Some(r) = n.receiver() {
                    self.infer_expr(&r, env, self_ty, scope);
                }
                self.infer_expr(&n.value(), env, self_ty, scope);
                Ty::Unknown
            }
            Node::CallAndWriteNode { .. } => {
                let n = node.as_call_and_write_node().unwrap();
                if let Some(r) = n.receiver() {
                    self.infer_expr(&r, env, self_ty, scope);
                }
                self.infer_expr(&n.value(), env, self_ty, scope);
                Ty::Unknown
            }
            Node::CallOperatorWriteNode { .. } => {
                let n = node.as_call_operator_write_node().unwrap();
                if let Some(r) = n.receiver() {
                    self.infer_expr(&r, env, self_ty, scope);
                }
                self.infer_expr(&n.value(), env, self_ty, scope);
                Ty::Unknown
            }
            Node::IndexOrWriteNode { .. }
            | Node::IndexAndWriteNode { .. }
            | Node::IndexOperatorWriteNode { .. } => Ty::Unknown,
            Node::SuperNode { .. } => {
                let n = node.as_super_node().unwrap();
                if let Some(args) = n.arguments() {
                    for a in &args.arguments() {
                        self.infer_expr(&a, env, self_ty, scope);
                    }
                }
                let kw = n.keyword_loc();
                self.maybe_goto_super((kw.start_offset(), kw.end_offset()), self_ty);
                Ty::Unknown
            }
            Node::ForwardingSuperNode { .. } => {
                if let Some(n) = node.as_forwarding_super_node() {
                    let loc = n.location();
                    self.maybe_goto_super((loc.start_offset(), loc.end_offset()), self_ty);
                }
                Ty::Unknown
            }
            Node::YieldNode { .. } => {
                let n = node.as_yield_node().unwrap();
                if let Some(args) = n.arguments() {
                    for a in &args.arguments() {
                        self.infer_expr(&a, env, self_ty, scope);
                    }
                }
                Ty::Unknown
            }
            Node::LambdaNode { .. } => {
                let n = node.as_lambda_node().unwrap();
                let mut lenv = env.clone();
                let saved_bp_len = self.block_params.len();
                let sink = self.census.then_some(&mut self.block_params);
                add_block_params(&mut lenv, n.parameters().as_ref(), sink);
                self.rebindable_block_depth += 1;
                if let Some(b) = n.body() {
                    self.infer_expr(&b, &mut lenv, self_ty, scope);
                }
                self.rebindable_block_depth -= 1;
                self.block_params.truncate(saved_bp_len);
                spill_block_writes(env, &lenv);
                Ty::Unknown
            }
            Node::BlockArgumentNode { .. } => Ty::Unknown,

            Node::MultiWriteNode { .. } => {
                let n = node.as_multi_write_node().unwrap();
                self.infer_expr(&n.value(), env, self_ty, scope);
                for t in n.lefts().iter().chain(n.rights().iter()) {
                    if let Some(lv) = t.as_local_variable_target_node() {
                        let name = String::from_utf8_lossy(lv.name().as_slice()).into_owned();
                        self.kill_constraint(&name);
                        env.insert(name, Ty::Unknown);
                    }
                }
                Ty::Unknown
            }
            Node::SplatNode { .. } => {
                let n = node.as_splat_node().unwrap();
                if let Some(e) = n.expression() {
                    self.infer_expr(&e, env, self_ty, scope);
                }
                Ty::Unknown
            }

            // Nested class/module/def in expression position: indexed (or
            // not) elsewhere; don't descend.
            Node::ClassNode { .. } | Node::ModuleNode { .. } | Node::SingletonClassNode { .. } => {
                Ty::Unknown
            }
            Node::DefNode { .. } => Ty::Sym,

            _ => Ty::Unknown,
        }
    }

    /// Bead ita-47y: `resolve_const_through_aliases` (not the plain
    /// `resolve_const`) so a reference reached only through a literal
    /// constant alias (`Interface::CompletionItemKind::FIELD` where
    /// `Interface = LanguageServer::Protocol::Interface`) types as
    /// `Ty::Class(id)` exactly like a direct reference to the target
    /// would — the SAME resolved class, not a special "aliased" type —
    /// which is what lets a bogus member reached through the alias
    /// legitimately raise E0101/E0103 downstream instead of silently
    /// typing `Ty::Unknown` forever.
    /// Bead ita-yh1: `check_const_ref` gets the UNTRIMMED `path` (leading
    /// `::` kept, not stripped here) — a cbase (`::`-prefixed) constant
    /// path resolves from the TOP LEVEL ONLY in real Ruby, and trimming it
    /// to a relative path before the diagnostic-path lookup let a
    /// same-named lexically-nested module SHADOW the real top-level target
    /// (`::Tapioca::TAPIOCA_DIR` inside `RubyLsp::Tapioca::ServerAddon`
    /// falsely resolving `Tapioca` to the sibling `RubyLsp::Tapioca` and
    /// missing the real member). `resolve_const_through_aliases` above
    /// already receives the untrimmed `path` and correctly treats a
    /// leading `::` as top-level-only (see its `strip_prefix("::")`); the
    /// fallback here must match, not silently re-widen to lexical scope.
    fn infer_const(&mut self, node: &Node<'_>, scope: &[String]) -> Ty {
        match const_path_str(node) {
            Some(path) => {
                if let Some(id) = self.index.resolve_const_through_aliases(scope, &path) {
                    Ty::Class(id)
                } else {
                    self.check_const_ref(scope, &path, node);
                    Ty::Unknown
                }
            }
            None => Ty::Unknown, // dynamic parent
        }
    }

    /// E0104 when a constant reference resolves nowhere (Warning). Also
    /// bead ita-vto's single choke point for the on-demand RBI fallback:
    /// Bead ita-47y (RBI-target extension): the alias's OWN target may
    /// live only in a vendorized gem `.rbi` (ruby-lsp's own `Interface =
    /// LanguageServer::Protocol::Interface`), never as a project
    /// `ClassId` — retry the RBI checks against the alias-expanded path.
    /// Suppression-only, same contract as `rbi_declares` beside its call
    /// site. Extracted from `check_const_ref` to keep that choke point
    /// under the complexity ceiling.
    fn alias_rbi_leaf_declares(
        &self,
        scope: &[String],
        path: &str,
        map: &std::collections::HashMap<String, Vec<std::path::PathBuf>>,
    ) -> bool {
        let Some(expanded) = self.index.expand_unresolved_alias_target(scope, path) else {
            return false;
        };
        crate::index::rbi_declares(&expanded, map)
            || crate::index::rbi_qualified_const_declares(&expanded, map)
    }

    /// every call site that can hit an unresolved constant (a bare
    /// reference here, a superclass at `scope_stmt`'s `ClassNode` arm)
    /// goes through this one function, so a client's `sorbet/rbi` never
    /// needs a second wiring point. Tried only after the project's own
    /// index has already failed, and only when a `sorbet/rbi` was actually
    /// discovered (`self.rbi_map`) — absent RBI, this is byte-for-byte the
    /// pre-bead behavior. `crate::index::rbi_declares` never mutates
    /// `self.index` (see its own doc comment for why) — a confirmed RBI
    /// hit only suppresses the diagnostic; the reference still types as
    /// `Ty::Unknown` in `infer_const` above, which is exactly as safe as
    /// an `open` class under invariant #1.
    /// Bead ita-3dg: the DIRECT branch never consulted
    /// `rbi_qualified_const_declares` — only `alias_rbi_leaf_declares`
    /// did, and only when `path` is ITSELF an unresolved literal alias
    /// (`expand_unresolved_alias_target` requires a `find_const_alias`
    /// hit on the failing segment). A qualified reference to a vendored
    /// gem's `T.let` const-write with NO alias in the way at all — the
    /// measured `RuboCop::Version::STRING` shape
    /// (`RuboCop::Version::STRING = T.let(T.unsafe(nil), String)` in
    /// `sorbet/rbi/gems/rubocop@*.rbi`, `RuboCop::Version` never a
    /// project class/module) — has no alias to chase, so
    /// `alias_rbi_leaf_declares` always returned `false` for it and only
    /// the bare `rbi_declares` (class/module header) ran, which a value
    /// write can never satisfy. Fix: try
    /// `rbi_qualified_const_declares(path, map)` directly too, exactly
    /// like `rbi_declares` beside it — same suppression-only contract
    /// (see that function's doc comment): a hit only silences E0104,
    /// `infer_const` still types the reference `Ty::Unknown` (invariant
    /// #1).
    /// Bead ita-hzd: `path` AS WRITTEN never matches a vendored RBI's
    /// compact-form header for a BARE reference written from inside real
    /// lexical nesting — Tapioca/Sorbet emits `class RubyLsp::Notification
    /// < ::RubyLsp::Message` at its full qualified path, never inferring
    /// that a project file's own `module RubyLsp; module Tapioca; class
    /// Addon; ...; end; end; end` would let a bare `Notification` written
    /// inside `Addon` find it lexically, exactly as `Notification` alone
    /// would from inside `RubyLsp` itself. `rbi_declares`/
    /// `rbi_qualified_const_declares`/`rbi_ancestor_declares` all take
    /// `path` literally, so a bare reference at 3+ levels of real nesting
    /// past the RBI's own declared owner missed every channel. Tried only
    /// after `path` as written already missed all three — mirrors
    /// `ProjectIndex::const_exists`'s own bare-name walk (innermost
    /// lexical scope outward), except each candidate PATH TEXT (never a
    /// `ClassId`) is retried against the RBI channels instead of the
    /// project index. Every one of those three channels keeps its own
    /// exact-string refilter unchanged (`rbi_declares`'s `f.path ==
    /// name`, `rbi_qualified_const_declares`'s `f.path == owner`) — this
    /// only widens WHICH full path gets tried, never how loosely a match
    /// counts, so an unrelated RBI name that merely shares a suffix or
    /// prefix with `path` still never matches. Suppression-only, same
    /// contract as every other RBI channel.
    fn nesting_expanded_rbi_declares(
        &self,
        scope: &[String],
        path: &str,
        map: &std::collections::HashMap<String, Vec<std::path::PathBuf>>,
    ) -> bool {
        scope.iter().rev().any(|level| {
            let candidate = format!("{level}::{path}");
            crate::index::rbi_declares(&candidate, map)
                || crate::index::rbi_qualified_const_declares(&candidate, map)
                || crate::index::rbi_ancestor_declares(self.index, &[], &candidate, map)
                || self.rbi_alias_edge_declares(&candidate, map)
        })
    }

    /// Bead ita-4wq: does `candidate` resolve through an alias EDGE
    /// written INSIDE the vendored RBI itself (`crate::index::rbi_alias_expand`
    /// — a qualified const-write whose RHS parses as a literal const
    /// path, e.g. `RubyLsp::Constant = LanguageServer::Protocol::Constant`)?
    /// The full chain this bead exists to complete: bead ita-hzd's
    /// nesting expansion produces `candidate` (`<enclosing
    /// scope>::<path as written>`), this bead recognizes `candidate`'s
    /// prefix as an RBI-internal alias and expands it to the real gem's
    /// namespace, and beads ita-3dg/ita-y0s's `rbi_declares`/
    /// `rbi_qualified_const_declares` resolve the member on the expanded
    /// path. Suppression-only, same contract as every other RBI channel:
    /// an unresolvable alias target (in the RBI or the project) simply
    /// leaves `candidate` exactly as unresolved as before this bead.
    fn rbi_alias_edge_declares(
        &self,
        candidate: &str,
        map: &std::collections::HashMap<String, Vec<std::path::PathBuf>>,
    ) -> bool {
        let Some(expanded) = crate::index::rbi_alias_expand(candidate, map) else {
            return false;
        };
        crate::index::rbi_declares(&expanded, map)
            || crate::index::rbi_qualified_const_declares(&expanded, map)
            || crate::index::rbi_ancestor_declares(self.index, &[], &expanded, map)
    }

    /// Every RBI fallback channel in one place (extracted from
    /// `check_const_ref` to keep that choke point under the complexity
    /// ceiling — same discipline `alias_rbi_leaf_declares` was born
    /// with). ALL of them are suppression-only — see each channel's own
    /// doc comment (`rbi_ancestor_declares` is W3 require/autoload: the
    /// reference may resolve through an EXTERNAL ancestor the project
    /// index can't see, like graphql's `GraphQLTypeNames` mixin behind
    /// `Types::BaseObject < GraphQL::Schema::Object`).
    fn rbi_fallback_declares(
        &self,
        scope: &[String],
        path: &str,
        map: &std::collections::HashMap<String, Vec<std::path::PathBuf>>,
    ) -> bool {
        crate::index::rbi_declares(path, map)
            || crate::index::rbi_qualified_const_declares(path, map)
            || self.alias_rbi_leaf_declares(scope, path, map)
            || crate::index::rbi_ancestor_declares(self.index, scope, path, map)
            || self.nesting_expanded_rbi_declares(scope, path, map)
            || self.rbi_alias_edge_declares(path, map)
    }

    /// Suppression-only: does a `self.const_missing` hook govern `path`?
    /// Such a namespace autovivifies constants at runtime — the reference
    /// resolves through the hook and the program runs clean — so its
    /// surface is unknowable by construction and an "unresolved constant"
    /// there reads absence of evidence as evidence of absence (AGENTS.md,
    /// binding; invariant #1).
    ///
    /// WHICH module Ruby asks is the whole question, and both answers below
    /// are measured, not reasoned:
    ///
    /// * QUALIFIED (`A::B`) — the IMMEDIATE parent, and only it. Nesting is
    ///   not inheritance: for `GP::Inner::Widget` MRI calls
    ///   `Inner.const_missing`, so a hook on `GP` never covers it.
    /// * BARE (`Widget`) — the innermost enclosing namespace (the cref),
    ///   and only it. An earlier version of this function claimed "a bare
    ///   `Foo` is not governed by any namespace's hook" and that was simply
    ///   FALSE: Ruby calls `const_missing` on the cref, so a bare reference
    ///   inside the very module that defines the hook resolves fine and was
    ///   being warned about (a live false positive, found by review
    ///   2026-09-17). The cref chain is NOT walked outward, measured:
    ///   with a hook on `Outer` and the reference inside `Outer::Inner`,
    ///   MRI raises `uninitialized constant Outer::Inner::Widget`. So the
    ///   innermost entry is consulted alone — walking outward would buy
    ///   silence Ruby does not give and cost real diagnostics.
    ///
    /// A `::`-prefixed reference is resolved from the root and no cref
    /// governs it, on either branch.
    ///
    /// The hook must be one this project really defines: `const_missing`
    /// lives in `CLASS_MODULE_ONLY_METHODS`, so `lookup_singleton` answers
    /// `Found` here exactly when project code wrote `def self.const_missing`,
    /// never for the inherited default.
    fn const_missing_namespace(&self, scope: &[String], path: &str) -> bool {
        let cbase = path.starts_with("::");
        let trimmed = path.trim_start_matches("::");
        let owner = match trimmed.rfind("::") {
            Some(idx) => {
                let parent = &trimmed[..idx];
                let scope: &[String] = if cbase { &[] } else { scope };
                self.index.resolve_const(scope, parent)
            }
            None if cbase => None,
            // The cref: innermost lexical namespace, already a full path.
            None => scope.last().and_then(|inner| self.index.resolve_const(&[], inner)),
        };
        let Some(pid) = owner else {
            return false;
        };
        matches!(
            self.index.lookup_singleton(pid, "const_missing"),
            MethodLookup::Found(..)
        )
    }

    fn check_const_ref(&mut self, scope: &[String], path: &str, node: &Node<'_>) {
        if self.index.resolve_const_through_aliases(scope, path).is_some() || self.index.const_exists(scope, path) {
            return;
        }
        // Bead ita-r8k: `path` is the exact same literal text an
        // enclosing `defined?(path)` guard already proved exists in the
        // branch currently being walked (`self.defined_guards`, pushed by
        // the `IfNode`/`UnlessNode` arms above). Suppression-only, same
        // contract as every RBI channel below: a hit never changes what
        // `infer_const` types the reference as (invariant #1).
        if self.defined_guards.iter().any(|g| g == path) {
            return;
        }
        if self.const_missing_namespace(scope, path) {
            return;
        }
        if let Some(map) = self.rbi_map {
            if self.rbi_fallback_declares(scope, path, map) {
                return;
            }
        }
        // Bead ita-yh1: `path` may itself be cbase-prefixed (`::Foo::BAR`)
        // now that `infer_const` no longer trims it before calling here —
        // strip the leading `::` before taking the first `::`-segment, or
        // `head` would be the empty string for every cbase path and a
        // cbase reference to a real core constant (`::String::SOMETHING`)
        // would wrongly fail this check.
        let head = path.trim_start_matches("::").split("::").next().unwrap_or(path);
        if is_known_core_constant(head) {
            return;
        }
        let loc = node.location();
        let suggestion = if self.silent {
            None
        } else {
            self.const_suggestion(path)
        };
        self.emit_with(
            loc.start_offset(),
            loc.end_offset(),
            E0104_UNRESOLVED_CONSTANT,
            Severity::Warning,
            format!("unresolved constant `{path}`"),
            suggestion,
        );
    }

    // -- flow narrowing (bead ita-u1t) ---------------------------------------

    /// Resolve an `is_a?` argument to the class it names, without emitting
    /// any diagnostic. The argument was already evaluated (and any E0104
    /// already emitted) by the ordinary `infer_expr` walk of the whole
    /// predicate call; this only re-derives which `ClassId` it resolved to,
    /// purely to compute the narrowed type — evaluating it a second time
    /// here must never duplicate a diagnostic.
    fn resolve_const_ty(&self, node: &Node<'_>, scope: &[String]) -> Option<ClassId> {
        let path = const_path_str(node)?;
        self.index.resolve_const(scope, &path)
    }

    /// Extract a flow-narrowing fact from an `if`/`unless` predicate:
    /// `x.is_a?(Foo)` (`x` a local var or parameter, `Foo` a resolvable
    /// project class) or `x.nil?`, with no other arguments; a bare
    /// truthy read of `x` itself (`if x`, ternary `x ? a : b`); or
    /// `x.nil? || <anything>` (bead ita-w9i, family a — the guards the
    /// original `is_a?`/`nil?`-only version of this function couldn't
    /// see). Any other shape — an ivar receiver, an unresolved/non-class
    /// argument, extra arguments, `kind_of?`/`instance_of?` — yields no
    /// fact. No fact is always safe: the branches just stay unrefined,
    /// same as before this bead.
    fn narrow_of(&self, predicate: &Node<'_>, scope: &[String]) -> Option<Narrow> {
        if let Some(or) = predicate.as_or_node() {
            return self.narrow_of_or(&or, scope);
        }
        if let Some(call) = predicate.as_call_node() {
            return self.narrow_of_call(&call, scope);
        }
        // Bare truthy read of a local var/param: `if x`, `unless x`,
        // ternary `x ? a : b` — no `.nil?`/`.is_a?` call at all (bead
        // ita-w9i). `x.nil?` is a call node above; this only matches a
        // plain variable read, so it never overlaps with that arm.
        let local = predicate.as_local_variable_read_node()?;
        let var = String::from_utf8_lossy(local.name().as_slice()).into_owned();
        Some(Narrow {
            var,
            kind: NarrowKind::Truthy,
        })
    }

    /// `a.nil? || b` (and its mirror `b || a.nil?`): the *only* way the
    /// whole `||` is false is for every disjunct to be false, so
    /// `a.nil?` being false survives through the `||` even though `||`
    /// being true says nothing about `a` (it could be `b` alone that
    /// made it true) — hence the asymmetric `NilCheckOrGuard` kind
    /// rather than reusing `NilCheck` outright.
    fn narrow_of_or(
        &self,
        or: &ruby_prism::OrNode<'_>,
        scope: &[String],
    ) -> Option<Narrow> {
        for side in [or.left(), or.right()] {
            if let Some(Narrow {
                var,
                kind: NarrowKind::NilCheck,
            }) = self.narrow_of(&side, scope)
            {
                return Some(Narrow {
                    var,
                    kind: NarrowKind::NilCheckOrGuard,
                });
            }
        }
        None
    }

    /// Call-shaped predicates: `x.nil?` with no arguments, or
    /// `x.is_a?(Foo)` with exactly one. Any other call shape yields no
    /// fact.
    fn narrow_of_call(
        &self,
        call: &ruby_prism::CallNode<'_>,
        scope: &[String],
    ) -> Option<Narrow> {
        let recv = call.receiver()?;
        let local = recv.as_local_variable_read_node()?;
        let var = String::from_utf8_lossy(local.name().as_slice()).into_owned();
        if call.name().as_slice() == b"nil?" && call.arguments().is_none() {
            return Some(Narrow {
                var,
                kind: NarrowKind::NilCheck,
            });
        }
        if call.name().as_slice() == b"is_a?" {
            let args = call.arguments()?;
            let mut it = args.arguments().iter();
            let arg = it.next()?;
            if it.next().is_some() {
                return None; // exactly one argument only
            }
            // A resolvable project class first (v0); a core class name
            // narrows to the core type itself (bead ita-2ve) — sound at
            // runtime by definition of `is_a?`, and gated on closed-world
            // so gem projects keep their exact v0 flow types (an
            // allowlisted arity check or return type flowing out of a
            // newly narrowed branch would otherwise drift the corpora
            let ty = match self.resolve_const_ty(&arg, scope) {
                Some(class) => Ty::Instance(class),
                None if self.closed_world() => {
                    const_path_str(&arg).and_then(|p| core_const_ty(&p))?
                }
                None => return None,
            };
            return Some(Narrow {
                var,
                kind: NarrowKind::IsA(ty),
            });
        }
        None
    }

    /// Bead ita-w2c: the type a `&&`'s LEFT operand proved for the
    /// expression `node` (see `Checker::narrowed_names`), if any. Keyed
    /// by NAME, not by span: the two `rack_app` occurrences in
    /// `rack_app.is_a?(Class) && rack_app < Rails::Engine` are distinct
    /// call nodes at distinct spans, but the same name. A local variable
    /// read and a receiverless, argument-less, block-less call are the
    /// only shapes the key can name — anything else is not an expression
    /// identity this walker can track, and gets no fact.
    fn narrowed_expr_ty(&self, node: &Node<'_>) -> Option<Ty> {
        let name = expr_name(node)?;
        self.narrowed_names
            .iter()
            .rev()
            .find(|(k, _)| *k == name)
            .map(|(_, t)| t.clone())
    }

    /// Bead ita-w2c: does `predicate` prove its SUBJECT is a class or
    /// module object — `x.is_a?(Class)` / `x.is_a?(Module)`? Returns the
    /// subject's name when `x` is an expression `narrowed_names` can key
    /// on. Both spellings really are the language's own answer: `Class`
    /// and `Module` objects are exactly the ones `is_a?` can prove for a
    /// receiver whose singleton method table is not modelled.
    ///
    /// The resolved-constant precedence matches `narrow_of_when`'s: a
    /// project that declares its own `Class`/`Module` constant shadows
    /// the core one and the guard proves nothing about class objects, so
    /// a resolvable project constant bails first. Deliberately NOT gated
    /// on `closed_world()` — like the `.extend` widening beside
    /// `check_call`'s receiver arm, a receiver read as `Unknown` can
    /// never fabricate a diagnostic under any world-openness, and both
    /// measured corpora run open-world.
    ///
    /// `kind_of?`/`instance_of?` and every combinator predicate
    /// (`x.is_a?(Class) && y.is_a?(Class)` as a LEFT operand is fine —
    /// it is the left operand itself that must be the `is_a?` call) stay
    ///   out: no measured site, no fact.
    fn class_object_guard(&self, predicate: &Node<'_>, scope: &[String]) -> Option<String> {
        let call = predicate.as_call_node()?;
        if call.name().as_slice() != b"is_a?" {
            return None;
        }
        let name = expr_name(&call.receiver()?)?;
        let args = call.arguments()?;
        let mut it = args.arguments().iter();
        let arg = it.next()?;
        if it.next().is_some() {
            return None;
        }
        let path = const_path_str(&arg)?;
        let core = path.trim_start_matches("::");
        if !matches!(core, "Class" | "Module") {
            return None;
        }
        // A lexical class literally named `Class`/`Module` shadows the
        // core one, and then the guard proves nothing about class
        // objects. A project REOPENING the core `Class`/`Module` (rails'
        // `active_support/core_ext/module/*` does exactly that) is the
        // core class itself — the index files it under that very path —
        // so the path, not mere resolvability, is the test.
        if let Some(id) = self.index.resolve_const(scope, &path) {
            if self.index.class(id).path.trim_start_matches("::") != core {
                return None;
            }
        }
        Some(name)
    }

    /// Dark census only: record this arm's verdict. A no-op unless the run
    /// was started through `check_file_dark` — the normal diagnostic path
    /// never pays for a push (the flag check folds away).
    fn dark_record(
        &mut self,
        loc: (usize, usize),
        receiver: &str,
        method: &str,
        verdict: DarkVerdict,
    ) {
        if self.dark {
            self.dark_recs.push(DarkSingleton {
                start: loc.0,
                end: loc.1,
                receiver: receiver.to_string(),
                method: method.to_string(),
                verdict,
            });
        }
    }

    /// `case <subject>; when <this clause>` narrowing fact (bead ita-w9i,
    /// family b): sound only when EVERY condition in the clause is a
    /// resolvable project or (closed-world) core class constant —
    /// `when A, B` means `subject.is_a?(A) || subject.is_a?(B)` under
    /// `Module#===`, so the branch narrows to `A | B`. A single
    /// unresolvable condition (`Range`, `Regexp`, splat, unresolved
    /// const, …) makes the whole clause unnarrowable — never partially
    /// narrow off of the conditions that did resolve.
    fn narrow_of_when(&self, when: &ruby_prism::WhenNode<'_>, scope: &[String]) -> Option<Ty> {
        let mut ty: Option<Ty> = None;
        for c in &when.conditions() {
            let class_ty = match self.resolve_const_ty(&c, scope) {
                Some(class) => Ty::Instance(class),
                None if self.closed_world() => {
                    const_path_str(&c).and_then(|p| core_const_ty(&p))?
                }
                None => return None,
            };
            ty = Some(match ty {
                Some(prev) => Ty::union(prev, class_ty),
                None => class_ty,
            });
        }
        ty
    }

    // -- call checking -------------------------------------------------------

    /// Bead ita-qst: does an argument node whose own span starts at
    /// `start` sit on the same physical line as a parsed `#: as <target>`
    /// inline-cast comment? Line-range containment, not equality — see
    /// `collect_cast_comments`'s doc comment for why a byte range (not a
    /// line number) is what gets stored.
    fn cast_comment_at(&self, start: usize) -> Option<&CastTarget> {
        self.cast_comments
            .iter()
            .find(|((line_start, line_end), _)| (*line_start..*line_end).contains(&start))
            .map(|(_, target)| target)
    }

    /// Apply an inline `#: as <target>` cast (bead ita-qst) to an
    /// already-inferred argument type, scoped to the ONE call argument
    /// whose source line carries the comment — `check_call`'s argument
    /// loop is the only caller, and it calls this once per argument
    /// immediately after inferring that argument's own type, so nothing
    /// here can leak to a sibling argument on a different line or to any
    /// later statement. `!nil` reuses the same `strip_nil` flow-narrowing
    /// already uses (Unknown stays Unknown — invariant #1: a cast can
    /// narrow a real type, never invent one). A named target resolves
    /// through `sorbet_sig::resolve_ret_ty` (core scalar or project
    /// class, the identical mapping `sig_fill` already reuses for
    /// `.returns(...)` text); an unresolvable name (garbage cast, or a
    /// class this project never declares) leaves `t` untouched — the
    /// call's diagnostics stay byte-identical to having no cast comment
    /// at all, never a guess.
    fn apply_cast_comment(&self, t: Ty, start: usize) -> Ty {
        match self.cast_comment_at(start) {
            Some(CastTarget::NotNil) => strip_nil(t),
            // `untyped` erases the type unconditionally (bead ita-j0z) —
            // never resolved through `resolve_ret_ty`, since erasure is
            // the whole point and there is no fallback to "leave it
            // alone" the way an unresolvable `Named` target has.
            Some(CastTarget::Untyped) => Ty::Unknown,
            Some(CastTarget::Named(name)) => {
                match crate::sorbet_sig::resolve_ret_ty(Some(name), self.index) {
                    Ty::Unknown => t,
                    resolved => resolved,
                }
            }
            None => t,
        }
    }

    fn check_call(
        &mut self,
        call: &CallNode<'_>,
        env: &mut Env,
        self_ty: SelfTy,
        scope: &[String],
    ) -> Ty {
        // Bead ita-w2c, the `RSpec` half of the asserted-raise softening:
        // `expect { <subject> }.to raise_error(...)` — the subject lives
        // inside the RECEIVER, so the arm has to be in place before the
        // receiver is inferred below, and only for the span of this one
        // walk (nothing about it may outlive the call that owns the
        // block).
        let rspec_subjects = rspec_raise_subject_spans(call);
        let saved_subject_len = self.asserted_subject_spans.len();
        self.asserted_subject_spans.extend(rspec_subjects);
        let recv_ty = match call.receiver() {
            Some(r) => {
                let t = self.infer_expr(&r, env, self_ty, scope);
                // `x.extend(M)` hands the object M's instance methods at
                // runtime; no model of singleton extension exists, so the
                // honest type for `x` from here on is Unknown (bead
                // ita-2ve). Local-variable case: closed-world-gated like
                // every other behavior change of that bead — v0 kept the
                // (wrongly) precise type.
                if call.name().as_slice() == b"extend" {
                    if self.closed_world() {
                        if let Some(local) = r.as_local_variable_read_node() {
                            let var = String::from_utf8_lossy(local.name().as_slice()).into_owned();
                            env.insert(var, Ty::Unknown);
                        }
                    }
                    // Bead ita-o8l.4: an ivar receiver of `.extend(M)`
                    // cannot reuse the local-variable mechanism above —
                    // an ivar is strictly more fragile than a local (any
                    // method on the object can reassign it), so per-flow
                    // narrowing would be unsound the moment a DIFFERENT
                    // method reads the same ivar. `ivar_ty` is already
                    // class-wide, not per-method (see its doc comment),
                    // so this widening survives across method boundaries
                    // by construction: it pushes into the SAME
                    // `ivar_capture` side channel `InstanceVariableWriteNode`
                    // uses below, active only during `ivar_ty`'s own
                    // silent whole-class walk. A single `Ty::Unknown`
                    // entry poisons `fold_ivar_writes`'s fold for this
                    // name permanently — pure suppression, never a new
                    // diagnostic under ANY circumstance (invariant #1),
                    // which is why this is deliberately NOT gated on
                    // `closed_world()` the way the local-variable case
                    // above is: that gate exists because OTHER behavior
                    // changes in bead ita-2ve (core-class narrowing) are
                    // only sound when gems can't invisibly monkeypatch
                    // core classes, but widening a receiver to Unknown
                    // can never fabricate a diagnostic regardless of
                    // world-openness — and both measured corpus sites
                    // (rails-corpus has a `Gemfile` but no
                    // `sorbet/rbi/gems`, so `closed_world()` is false
                    // there) need it to fire unconditionally to close at
                    // all.
                    if let Some(ivar) = r.as_instance_variable_read_node() {
                        if let SelfTy::Instance(c) = self_ty {
                            let name = String::from_utf8_lossy(ivar.name().as_slice()).into_owned();
                            if let Some((cap_class, values)) = self.ivar_capture.as_mut() {
                                if *cap_class == c {
                                    values.entry(name).or_default().push(Ty::Unknown);
                                }
                            }
                        }
                    }
                }
                t
            }
            None => self_ty.as_ty(),
        };
        // Bead ita-w2c: the enclosing `&&`'s left operand may have
        // proven this very expression is a class/module object, whose
        // method table no `Ty` models — the narrowed reading replaces
        // whatever the receiver's own inference produced (the walk above
        // still happened, so diagnostics INSIDE the receiver are
        // untouched).
        let recv_ty = call
            .receiver()
            .and_then(|r| self.narrowed_expr_ty(&r))
            .unwrap_or(recv_ty);
        let name = String::from_utf8_lossy(call.name().as_slice()).into_owned();
        // The `expect` block has been walked: the RSpec arm ends here, so
        // a sibling argument or the matcher call can never ride it.
        self.asserted_subject_spans.truncate(saved_subject_len);

        // Arguments.
        let mut pos_args: Vec<(Ty, (usize, usize), Option<String>)> = Vec::new();
        let mut kw_args: Vec<KeywordArg> = Vec::new();
        let mut nil_literals: Vec<(usize, usize)> = Vec::new();
        let mut sorbet_args_known = call.block().is_none();
        let mut exact_arity = true;
        if let Some(args) = call.arguments() {
            for a in &args.arguments() {
                match &a {
                    Node::SplatNode { .. } | Node::ForwardingArgumentsNode { .. } => {
                        exact_arity = false;
                        sorbet_args_known = false;
                        self.infer_expr(&a, env, self_ty, scope);
                    }
                    Node::KeywordHashNode { .. } => {
                        // `**kwargs` can collapse into a positional hash
                        // when the callee has no keyword params: positional
                        // arity is no longer knowable here.
                        exact_arity = false;
                        let Some(hash) = a.as_keyword_hash_node() else {
                            sorbet_args_known = false;
                            self.infer_expr(&a, env, self_ty, scope);
                            continue;
                        };
                        for element in &hash.elements() {
                            if let Some(assoc) = element.as_assoc_node() {
                                if let Some(key) = assoc.key().as_symbol_node() {
                                    let value = assoc.value();
                                    let loc = value.location();
                                    let ty = self.infer_expr(&value, env, self_ty, scope);
                                    let ty = self.apply_cast_comment(ty, loc.start_offset());
                                    let name = String::from_utf8_lossy(key.unescaped()).into_owned();
                                    if matches!(value, Node::NilNode { .. }) {
                                        nil_literals.push((loc.start_offset(), loc.end_offset()));
                                    }
                                    if kw_args.iter().any(|(previous, _, _)| previous == &name) {
                                        sorbet_args_known = false;
                                    }
                                    kw_args.push((
                                        name,
                                        ty,
                                        (loc.start_offset(), loc.end_offset()),
                                    ));
                                } else {
                                    sorbet_args_known = false;
                                    self.infer_expr(&element, env, self_ty, scope);
                                }
                            } else {
                                sorbet_args_known = false;
                                self.infer_expr(&element, env, self_ty, scope);
                            }
                        }
                    }
                    Node::BlockArgumentNode { .. } => { sorbet_args_known = false; }
                    _ => {
                        let loc = a.location();
                        let t = self.infer_expr(&a, env, self_ty, scope);
                        // Bead ita-qst: `#: as <target>` on this
                        // argument's own line overrides its inferred
                        // type for THIS call only — see
                        // `apply_cast_comment`'s doc comment.
                        let t = self.apply_cast_comment(t, loc.start_offset());
                        // Plain (non-interpolated) string literal only —
                        // the E0106 schema-cast check (bead ita-yho) must
                        // never guess at a variable's or interpolation's
                        // runtime content.
                        let lit = a
                            .as_string_node()
                            .map(|s| String::from_utf8_lossy(s.unescaped()).into_owned());
                        if matches!(a, Node::NilNode { .. }) {
                            nil_literals.push((loc.start_offset(), loc.end_offset()));
                        }
                        pos_args.push((t, (loc.start_offset(), loc.end_offset()), lit));
                    }
                }
            }
        }
        // Block: check its body with outer locals visible. `self` inside
        // it is only knowable when the method RECEIVING the block provably
        // never rebinds it (bead ita-uye).
        if let Some(block) = call.block() {
            if let Some(b) = block.as_block_node() {
                let mut benv = env.clone();
                let saved_bp_len = self.block_params.len();
                let sink = self.census.then_some(&mut self.block_params);
                add_block_params(&mut benv, b.parameters().as_ref(), sink);
                let rebindable = !self.block_keeps_lexical_self(&recv_ty, &name);
                self.rebindable_block_depth += usize::from(rebindable);
                // Bead ita-w2c, the minitest half: `assert_raises(...)`
                // (`assert_raise`) takes the raising code as its BLOCK —
                // the direct statements are the subject, and their
                // exception is the asserted behavior. Armed around this
                // block walk only.
                let minitest_subjects = (name == "assert_raises" || name == "assert_raise")
                    .then(|| b.body().map(|body| direct_statement_call_spans(&body)))
                    .flatten()
                    .unwrap_or_default();
                let saved_subject_len = self.asserted_subject_spans.len();
                self.asserted_subject_spans.extend(minitest_subjects);
                if let Some(body) = b.body() {
                    self.infer_expr(&body, &mut benv, self_ty, scope);
                }
                self.asserted_subject_spans.truncate(saved_subject_len);
                self.rebindable_block_depth -= usize::from(rebindable);
                self.block_params.truncate(saved_bp_len);
                spill_block_writes(env, &benv);
            }
        }

        let msg_loc = call.message_loc().map_or_else(
            || span_of_call(call),
            |l| (l.start_offset(), l.end_offset()),
        );
        let typed_args = CallTypeArgs {
            positional: &pos_args,
            keywords: sorbet_args_known.then_some(kw_args.as_slice()),
            nil_literals: &nil_literals,
        };

        // E0108 is decided from the operand NODES, independently of
        // which method table below resolves the operator.
        self.check_operand_types(call, &recv_ty, env);

        // Bead ita-w2c: this call IS the direct subject of an asserted raise
        // (`assert_raises { ... }` / `expect { ... }.to raise_error(...)`)
        // — the exception it raises, arity included, is the ASSERTED
        // behavior and not a defect. Span-keyed, so a call nested any
        // deeper inside the block keeps firing.
        if self
            .asserted_subject_spans
            .iter()
            .any(|s| *s == span_of_call(call))
        {
            self.tally_inconclusive(None);
            self.note_unknown_origin(call, UnkOrigin::ProjectRet, None);
            return Ty::Unknown;
        }

        // Self-send inside a block whose receiving call could rebind
        // `self` (`instance_exec`-style): the receiver here may not be
        // `self_ty` at runtime, so NOTHING the walk below resolves
        // against `self_ty` is conclusive — not a missing method, and
        // not the arity/sig of a method that happens to exist there
        // either (bead ita-w2c). The guard therefore sits ABOVE the
        // `lookup_method` dispatch, where the `Found`/arity/sig path
        // consults it too; it used to live only in the `NotFound` arm,
        // which let `body html` inside `Mail::Part.new { ... }` be
        // accused of the ENCLOSING builder's own zero-argument `body`
        // while the block's real `self` is the part being built. A block
        // proven to keep lexical self never raised the count (bead
        // ita-uye) — see `rebindable_block_depth`.
        //
        // Bead ita-slf (2026-09-21): an EXPLICIT `self` receiver in such
        // a block is the same call in a different spelling — `self` IS
        // the rebound object, so `self.send_shortcut = value` inside
        // `base.define_method(:chat_send_shortcut=) { |v| ... }`
        // (discourse `plugins/chat/lib/chat/user_option_extension.rb:116`)
        // and `self.description = "..."` inside `Class.new(Command) do`
        // (`migrations/core/lib/migrations/cli/bootstrap.rb:62`) name the
        // UserOption instance and the anonymous subclass, never the
        // lexically enclosing class the walk types them against. Both
        // were census residue records on receivers that never see the
        // call. A NAMED receiver (`helper.step`) is untouched: rebinding
        // `self` does not move a local, which is exactly what
        // `testdata/rebindable_guard/
        // explicit_receiver_in_rebindable_block_accuses.rb` pins.
        if self.rebindable_block_depth > 0
            && call.receiver().is_none_or(|r| r.as_self_node().is_some())
        {
            self.tally_inconclusive(None);
            self.note_unknown_origin(call, UnkOrigin::ProjectRet, None);
            return Ty::Unknown;
        }

        match recv_ty {
            Ty::Instance(c) => match self.index.lookup_method_rbi(c, &name, self.rbi_map) {
                MethodLookup::Found(m, owner) => {
                    self.tally(Bucket::Resolved);
                    let m = m.clone();
                    self.maybe_goto(msg_loc, &m, &name);
                    // An abstract stub (`raise NotImplementedError` body)
                    // never runs on a family instance: any member
                    // redefining the name supplies the signature that
                    // actually dispatches, and the receiver here is the
                    // base itself (MRO-first) only because the call sits
                    // in the base's own body. No family redefinition means
                    // the stub IS the effective method — its arity check
                    // is then a real latent-ArgumentError guard.
                    if exact_arity
                        && !(m.abstract_stub
                            && self.index.descendant_defines(owner, &name, false))
                    {
                        self.check_arity(&m, &name, pos_args.len(), msg_loc);
                    }
                    self.check_sig_args(&m, &name, &typed_args, Some(owner), scope);
                    if let Some(col_type) = &m.schema_col_type {
                        self.check_schema_cast(col_type, &name, &pos_args);
                    }
                    let ret = self.method_return(c, &name, false, &m, scope);
                    if ret == Ty::Unknown {
                        self.note_unknown_origin(call, UnkOrigin::ProjectRet, self.last_ret_cause);
                    }
                    ret
                }
                MethodLookup::NotFound => if let Some(cm) = core_method(CoreClass::Object, &name) {
                    self.tally(Bucket::Core);
                    if exact_arity {
                        self.check_core_arity(cm, &name, pos_args.len(), msg_loc);
                    }
                    // Same knowledge gap as the concrete-core arm
                    // below (bead ita-au5): `.tap`/`.send`/`.then` on
                    // a project instance resolves through the `Object`
                    // core table, so an Unknown return here is a core
                    // return we never modeled — not an unclassified
                    // receiver.
                    let ret = core_ret_to_ty(cm.ret, &Ty::Instance(c));
                    if ret == Ty::Unknown {
                        self.note_unknown_origin(call, UnkOrigin::CoreRet, None);
                    }
                    ret
                } else {
                    // Bead ita-w2c: `if respond_to?(:setup); setup; end`
                    // — `self` was proven to answer exactly this name in
                    // the branch being walked, so the receiverless call
                    // cannot be a `NoMethodError`. Suppression-only: the
                    // call types as `Unknown`, never as a resolved
                    // signature inferred from the guard.
                    if call.receiver().is_none()
                        && self.respond_to_guards.iter().any(|g| g == &name)
                    {
                        self.tally_inconclusive(None);
                        self.note_unknown_origin(call, UnkOrigin::ProjectRet, None);
                        return Ty::Unknown;
                    }
                    // Self-send inside a block whose receiving call
                    // could rebind `self` is already handled ABOVE the
                    // dispatch (bead ita-w2c moved it there so the
                    // Found/arity/sig path consults it too).
                    self.tally(Bucket::Diagnosed);
                    self.note_unknown_origin(call, UnkOrigin::ProjectRet, None);
                    let path = &self.index.class(c).path;
                    let message = format!("undefined method `{name}` for `{path}`");
                    let suggestion = if self.silent {
                        None
                    } else {
                        self.method_suggestion(c, &name)
                    };
                    self.emit_with(
                        msg_loc.0,
                        msg_loc.1,
                        E0101_UNKNOWN_METHOD,
                        Severity::Error,
                        message,
                        suggestion,
                    );
                    Ty::Unknown
                },
                MethodLookup::Inconclusive => if let Some(ty) = self.rbi_escalate(c, &name, false, &typed_args) {
                    ty
                } else {
                    let blocker = self.index.inconclusive_reason(c, false);
                    self.tally_inconclusive(blocker);
                    self.tally_ar_base(blocker, c);
                    self.note_unknown_origin(call, UnkOrigin::ProjectRet, None);
                    Ty::Unknown
                },
            },
            Ty::Class(c) => {
                if name == "new" {
                    // Bead ita-h6l (mechanism A): `Class.new(Superclass)`
                    // and `Struct.new(:a, :b)` do not return an instance
                    // of the receiver — they build and return a brand-new
                    // ANONYMOUS class/struct. `initialize`'s arity here
                    // would be `Class#initialize`/`Struct#initialize`
                    // (optional superclass; variadic + `keyword_init:`),
                    // neither modeled by this checker, and the resulting
                    // value is never `Instance(Class)`/`Instance(Struct)`
                    // — treating it as one cascaded every subsequent call
                    // (`.new` on the anonymous struct, `.mailer_name` on
                    // the anonymous class) into false E0101/E0102, the
                    // dominant FP family measured against rails/rails.
                    // Detection is by the receiver's resolved PATH, not
                    // "is core": a project that reopens `Class`/`Struct`
                    // still gets this carve-out (invariant #1) — the
                    // anonymous-class-construction semantics are a
                    // language feature, not something a reopening
                    // changes. Silence in everything downstream is
                    // exactly the fail-closed contract: no arity check,
                    // `Ty::Unknown` result.
                    let path = self.index.class(c).path.as_str();
                    if path == "Class" || path == "Struct" {
                        self.tally(Bucket::Core);
                        self.note_unknown_origin(call, UnkOrigin::CoreRet, None);
                        return Ty::Unknown;
                    }

                    // Bead ita-6bq: `Class.new(args)` used to always model
                    // `initialize`'s arity, even when the class defines its
                    // OWN `self.new` — the real dispatch target. A project
                    // (or `rails/activesupport`'s `DeprecationProxy`) that
                    // overrides `self.new` to do argument-shuffling before
                    // delegating to `allocate`/`initialize` makes
                    // `initialize`'s arity a lie about what `.new(...)`
                    // actually accepts. `lookup_singleton_own` (see
                    // `index.rs`; not the plain `lookup_singleton`, and
                    // not the `_rbi` variant) walks the same ancestry/
                    // `extend` chain the rest of this checker already uses
                    // for singleton dispatch, so "own" here means "the
                    // project itself defines it somewhere in `c`'s MRO" —
                    // but unlike plain `lookup_singleton`, it SKIPS an
                    // ancestor whose openness is a `declarations/gems.rbi`
                    // `DeclaredExternal` reopening (bead ita-k9j entrega
                    // 2's precedent: a gem base class's own `.new` never
                    // changes the arity contract a project's own
                    // `initialize` establishes — measured necessary, not
                    // optional: without the skip, EVERY ActiveRecord
                    // model's `.new` would go silently `Inconclusive`,
                    // since `ActiveRecord::Base` sits in every one of
                    // their ancestor chains). It is also deliberately not
                    // the `_rbi` variant: `soften_not_found` treats `new`
                    // as a Class/Module builtin — see
                    // `kernel_object_singleton_method`'s
                    // `CLASS_MODULE_ONLY_METHODS` — and would turn every
                    // closed-ancestry "no own `self.new`" class into a
                    // false `Inconclusive`, hiding the very distinction
                    // this bead needs.
                    //
                    // `Found`: that signature — not `initialize`'s — now
                    // governs arity. No separate "is this shape modelable"
                    // gate is needed: `check_arity`'s own `m.arity_unknown
                    // || m.rest` guard already no-ops for a variadic/
                    // forwarding self.new (`*args`, `...`), and
                    // `check_sig_args` only ever fires when `m.sig` is
                    // `Some` (an RBS `#:` comment), which a `**kwargs`-only
                    // shape leaves untouched either way — the exact same
                    // reliance on their internal guards every OTHER
                    // singleton-method dispatch in this file already uses
                    // (see the plain `lookup_singleton_rbi` arm just below,
                    // which never pre-filters shape either). A prototype
                    // gate here duplicating `m.rest`/`m.arity_unknown` was
                    // proven mutation-blind (never changed an observable
                    // diagnostic, only the census tally) and removed.
                    //
                    // `NotFound`: ancestry fully closed, no project-defined
                    // `self.new` anywhere in it — provably no own `self.new`
                    // is in play, so this falls through to the existing
                    // `initialize`-based match below, UNCHANGED: every class
                    // without its own `self.new` keeps byte-identical
                    // behavior.
                    //
                    // `Inconclusive`: the ancestry itself can't prove one
                    // way or the other (e.g. a class-body block like
                    // `instance_methods.each { |m| undef_method m }` opens
                    // the class — ita-d0j — before the walk ever reaches a
                    // subclass's own `initialize`). This must NOT fall
                    // through to the `initialize` match: an invisible
                    // ancestor `self.new` may exist, so `initialize`'s
                    // arity proves nothing about what `.new(...)` really
                    // accepts. Measured real-world shape: rails/
                    // activesupport's actual `DeprecationProxy` opens
                    // itself exactly this way (`instance_methods.each`
                    // block undefining most of `Object`'s surface) — a
                    // subclass with its own `initialize` used to get a
                    // fabricated E0102 from that unrelated arity. Silence
                    // (invariant #1), same census treatment as every other
                    // genuinely ancestry-blocked singleton lookup in this
                    // file (`tally_inconclusive` + `tally_ar_base`).
                    match self.index.lookup_singleton_own(c, "new") {
                        MethodLookup::Found(m, owner) => {
                            self.tally(Bucket::Resolved);
                            let m = m.clone();
                            self.maybe_goto(msg_loc, &m, "new");
                            if exact_arity {
                                self.check_arity(&m, "new", pos_args.len(), msg_loc);
                            }
                            self.check_sig_args(&m, "new", &typed_args, Some(owner), scope);
                            return Ty::Instance(c);
                        }
                        MethodLookup::Inconclusive => {
                            let blocker = self.index.inconclusive_reason(c, true);
                            self.tally_inconclusive(blocker);
                            self.tally_ar_base(blocker, c);
                            self.note_unknown_origin(call, UnkOrigin::ProjectRet, None);
                            return Ty::Instance(c);
                        }
                        MethodLookup::NotFound => {}
                    }
                    match self.index.lookup_method_rbi(c, "initialize", self.rbi_map) {
                        MethodLookup::Found(m, owner) => {
                            self.tally(Bucket::Resolved);
                            let m = m.clone();
                            self.maybe_goto(msg_loc, &m, "initialize");
                            if exact_arity {
                                self.check_arity(&m, "new", pos_args.len(), msg_loc);
                            }
                            self.check_sig_args(&m, "new", &typed_args, Some(owner), scope);
                        }
                        MethodLookup::NotFound => {
                            // Bead ita-gjb: ancestry fully closed with no
                            // `initialize` anywhere is NOT proof the real
                            // constructor takes 0 args. `Object#initialize`
                            // only applies when nothing else intercepts
                            // `.new`'s dispatch, and this checker never
                            // models the C-level constructors of classes it
                            // has no inventory for (a project reopening the
                            // checker's own external-name allowlist missed
                            // — the historical FPs were `Time`, `IPAddr`,
                            // `FastImage` — or a plain class this checker
                            // is simply wrong to assume nothing else ever
                            // defines `initialize` for). Same root as
                            // ita-h6l mechanism A/B, generalized to every
                            // `NotFound` verdict on `initialize`: silence,
                            // never a fabricated 0-arg arity check
                            // (invariant #1). `MethodLookup::Found` above
                            // is untouched — a project-defined or
                            // ancestor-inherited `initialize` still gets
                            // full arity/sig checking.
                            self.tally(Bucket::Inconclusive);
                            self.note_unknown_origin(call, UnkOrigin::ProjectRet, None);
                        }
                        MethodLookup::Inconclusive => {
                            if self.rbi_escalate(c, "initialize", false, &typed_args).is_none() {
                                let blocker = self.index.inconclusive_reason(c, true);
                                self.tally_inconclusive(blocker);
                                self.tally_ar_base(blocker, c);
                                self.note_unknown_origin(call, UnkOrigin::ProjectRet, None);
                            }
                        }
                    }
                    return Ty::Instance(c);
                }
                match self.index.lookup_singleton_rbi(c, &name, self.rbi_map) {
                    MethodLookup::Found(m, _) => {
                        self.tally(Bucket::Resolved);
                        let m = m.clone();
                        self.maybe_goto(msg_loc, &m, &name);
                        if exact_arity {
                            self.check_arity(&m, &name, pos_args.len(), msg_loc);
                        }
                        self.check_sig_args(&m, &name, &typed_args, Some(c), scope);
                        let ret = self.method_return(c, &name, true, &m, scope);
                        if ret == Ty::Unknown {
                            self.note_unknown_origin(
                                call,
                                UnkOrigin::ProjectRet,
                                self.last_ret_cause,
                            );
                        }
                        ret
                    }
                    // Class objects have a large builtin surface (name,
                    // ancestors, ...): never unknown-method here.
                    MethodLookup::Inconclusive => if let Some(ty) = self.rbi_escalate(c, &name, true, &typed_args) {
                        ty
                    } else {
                        let blocker = self.index.inconclusive_reason(c, true);
                        let receiver = self.index.class(c).path.clone();
                        // bead ita-tail: name-keyed label. When the name is
                        // the core bare-call tail, the silence is INVENTORY
                        // silence — record it as such instead of the
                        // unattributable `inconclusive_no_blocker`, so the
                        // JSONL self-describes why the site never accuses.
                        let verdict = if kernel_bare_call_method(&name) {
                            DarkVerdict::KnownTail
                        } else {
                            DarkVerdict::Open(blocker.map_or_else(
                                // Chain closed, lookup still Inconclusive: the
                                // missing Class/Module tail surface (`extend`,
                                // `include`, `name`, ...) — the census's own
                                // measurement of that inventory gap.
                                || "inconclusive_no_blocker".into(),
                                |b| format!("{b:?}"),
                            ))
                        };
                        self.dark_record(msg_loc, &receiver, &name, verdict);
                        self.tally_inconclusive(blocker);
                        self.tally_ar_base(blocker, c);
                        self.note_unknown_origin(call, UnkOrigin::ProjectRet, None);
                        Ty::Unknown
                    },
                    // THE CLASS-OBJECT FLIP (2026-09-21). A conclusive
                    // `NotFound` here is a certain `NoMethodError` — the
                    // receiver IS a class object whose whole singleton
                    // surface this index read (its own ancestry's
                    // singleton methods, every `extend`ed module's
                    // ancestry, project reopenings of
                    // `Class`/`Module`/`Object`/`Kernel`, the core
                    // bare-call tail, the stdlib singleton inventory, the
                    // lock-gated mocking-gem names). `lookup_singleton`
                    // returns Inconclusive at any open ancestor, and can
                    // return NotFound only after a complete chain misses.
                    // `lookup_singleton_rbi` can only soften that verdict.
                    // Rechecking the census blocker here was redundant:
                    // an open receiver never reaches this arm (CO-R).
                    //
                    // This is the dark census's `ClosedNotFound` residue.
                    // At the flip commit the public-corpus residue was
                    // rails 1 / mastodon 0 / discourse 7, every one of
                    // them read at its byte offset and proven to raise
                    // (`scripts/public-baseline/README.md`); the
                    // populations that used to sit here — block-nested
                    // class definitions, `Object`/`Kernel` core-ext
                    // reopenings, transitive `extend` ancestry,
                    // `include Singleton`, sclass-includes, def-body
                    // `eval`, `self` inside a rebindable block, generated
                    // string source, gem namespaces and bundled-gem
                    // Kernel functions — are each a NAMED reason now, and
                    // each has a mutant in
                    // `scripts/class-object-flip-mutants.sh`.
                    MethodLookup::NotFound => {
                        let receiver = self.index.class(c).path.clone();
                        self.dark_record(msg_loc, &receiver, &name, DarkVerdict::ClosedNotFound);
                        self.tally(Bucket::Diagnosed);
                        self.note_unknown_origin(call, UnkOrigin::ProjectRet, None);
                        let message =
                            format!("undefined method `{name}` for class `{receiver}`");
                        let suggestion = if self.silent {
                            None
                        } else {
                            self.singleton_method_suggestion(c, &name)
                        };
                        self.emit_with(
                            msg_loc.0,
                            msg_loc.1,
                            E0101_UNKNOWN_METHOD,
                            Severity::Error,
                            message,
                            suggestion,
                        );
                        Ty::Unknown
                    }
                }
            }
            ref t @ (Ty::Int
            | Ty::Float
            | Ty::Str
            | Ty::Sym
            | Ty::Nil
            | Ty::Bool
            | Ty::Array(_)
            | Ty::Hash(_, _)) => {
                let cc = core_class_of(t).expect("concrete core ty maps to a core class");
                let m = core_method(cc, &name).or_else(|| core_method(CoreClass::Object, &name));
                if let Some(cm) = m {
                    self.tally(Bucket::Core);
                    if exact_arity {
                        self.check_core_arity(cm, &name, pos_args.len(), msg_loc);
                    }
                    let args: Vec<Ty> = pos_args.iter().map(|(ty, _, _)| ty.clone()).collect();
                    let shape = CoreCallShape {
                        args: exact_arity.then_some(args.as_slice()),
                        int_literal: first_int_literal(call),
                    };
                    let ret = core_call_ret(cc, &name, cm.ret, t, &shape);
                    if ret == Ty::Unknown {
                        self.note_unknown_origin(call, UnkOrigin::CoreRet, None);
                    }
                    ret
                } else {
                    // v0 contract was "core_complete = false: silence".
                    // Bead ita-2ve adds the closed-world conclusion: E0101
                    // when (a) closed-world is wired on, (b) the project
                    // never reopened the class, and (c) the method is in
                    // neither the allowlist (how we got here) nor the
                    // generated inventory. w12 closure adds (d): under
                    // closed-world-via-Tapioca, no gem RBI reopening of
                    // the receiver's core namespaces declares it either —
                    // a declared `String#squish` exists, exactly like an
                    // inventory hit. Any miss = silence, still.
                    if self.core_unknown_is_conclusive(cc)
                        && !core_inventory_names(cc)
                            .iter()
                            .any(|cls| core_inventory_has(cls, &name))
                        && !self.rbi_core_declares(cc, &name)
                    {
                        let message = format!(
                            "undefined method `{name}` for `{}`",
                            ty_name(t, self.index)
                        );
                        self.emit(
                            msg_loc.0,
                            msg_loc.1,
                            E0101_UNKNOWN_METHOD,
                            Severity::Error,
                            message,
                        );
                        self.tally(Bucket::Diagnosed);
                    } else {
                        self.tally_inconclusive(None);
                    }
                    // Unmodeled core method and unmodeled core return
                    // are the same knowledge gap for the lever this
                    // census is deciding (bead ita-au5): either way,
                    // teaching the checker this method's real return
                    // type closes the blind spot.
                    self.note_unknown_origin(call, UnkOrigin::CoreRet, None);
                    Ty::Unknown
                }
            }
            Ty::Unknown => {
                self.tally_unknown_receiver(call);
                self.collect_constraint(call, &name, msg_loc);
                Ty::Unknown
            }
            Ty::Union(_) => {
                self.tally_unknown_receiver(call);
                Ty::Unknown
            }
        }
    }

    /// Bead ita-au5: classify why `node` (an expression that inferred
    /// `Ty::Unknown`/`Ty::Union`, or the receiver of a call that did)
    /// carries no type. `None` (implicit self with an Unknown self type,
    /// e.g. a bare top-level call) and any node shape not explicitly
    /// recognized fall to `Other`. Read-only — never mutates census
    /// state, so `Checker::tally_unknown_receiver`'s caller can call this
    /// and then increment `stats` separately.
    fn unknown_origin(&self, node: Option<&Node<'_>>) -> UnkOrigin {
        let Some(recv) = node else {
            return UnkOrigin::Other;
        };
        if let Some(local) = recv.as_local_variable_read_node() {
            let name = String::from_utf8_lossy(local.name().as_slice()).into_owned();
            if self.block_params.iter().any(|p| p == &name) || is_numbered_block_param(&name) {
                return UnkOrigin::BlockParam;
            }
            if self.method_params.iter().any(|p| p == &name) {
                return UnkOrigin::Param;
            }
            return self.local_origin.get(&name).copied().unwrap_or(UnkOrigin::Other);
        }
        if recv.as_it_local_variable_read_node().is_some() {
            return UnkOrigin::BlockParam;
        }
        if recv.as_instance_variable_read_node().is_some() {
            return UnkOrigin::Ivar;
        }
        if recv.as_constant_read_node().is_some() || recv.as_constant_path_node().is_some() {
            return UnkOrigin::Const;
        }
        if let Some(inner_call) = recv.as_call_node() {
            return self
                .unknown_call_src
                .get(&span_of_call(&inner_call))
                .map_or(UnkOrigin::Other, |(origin, _)| *origin);
        }
        UnkOrigin::Other
    }

    /// Bead ita-au5: record why `call` itself resolved to `Ty::Unknown`
    /// (project ret / core ret / dead chain), so an outer call that
    /// chains off `call` can attribute its own `unknown_receiver` hit to
    /// the real upstream cause via `unknown_origin`'s `CallNode` arm.
    /// `cause` (bead ita-mv5) is only ever `Some` at the two call sites
    /// where `origin` is `ProjectRet` AND a `MethodLookup::Found` callee
    /// actually ran through `method_return` — every other site (core ret,
    /// dead chain, and the `Inconclusive`/`NotFound` no-callee project-ret
    /// arms) passes `None`, which `tally_unknown_receiver` reads as
    /// "unresolved: no callee to blame a parameter/ivar/const on". Zero
    /// cost outside `ita check --stats` (`census` gate) and inert on
    /// nested silent re-walks (`!self.silent` gate — a foreign method
    /// body pulled in by `method_return`/`ivar_ty` never belongs to the
    /// file actually being censused).
    fn note_unknown_origin(
        &mut self,
        call: &CallNode<'_>,
        origin: UnkOrigin,
        cause: Option<RetCause>,
    ) {
        if self.census && !self.silent {
            self.unknown_call_src
                .insert(span_of_call(call), (origin, cause));
        }
    }

    /// Bead ita-mv5: `RetCause` sibling of `unknown_origin`'s `CallNode`
    /// arm — only meaningful when that arm would classify `node` as
    /// `UnkOrigin::ProjectRet`; any other shape has no stored cause.
    fn unknown_cause(&self, node: Option<&Node<'_>>) -> Option<RetCause> {
        let inner_call = node?.as_call_node()?;
        self.unknown_call_src
            .get(&span_of_call(&inner_call))
            .and_then(|(_, cause)| *cause)
    }

    /// Bead ita-au5: replaces the two `self.tally(Bucket::UnknownReceiver)`
    /// call sites in `check_call` — tallies the pre-existing bucket, then
    /// (census mode only) classifies `call`'s own receiver and increments
    /// the matching `unk_*` sub-bucket. `call` itself just proved to be a
    /// dead call (its own receiver was `Unknown`/`Union`), so it is also
    /// recorded as a `Chain` origin for whatever outer call chains off it.
    fn tally_unknown_receiver(&mut self, call: &CallNode<'_>) {
        self.tally(Bucket::UnknownReceiver);
        if !self.census || self.silent {
            return;
        }
        let recv = call.receiver();
        match self.unknown_origin(recv.as_ref()) {
            UnkOrigin::Param => self.stats.unk_param += 1,
            UnkOrigin::BlockParam => self.stats.unk_block_param += 1,
            UnkOrigin::CoreRet => self.stats.unk_core_ret += 1,
            UnkOrigin::ProjectRet => {
                self.stats.unk_project_ret += 1;
                let cause = self.unknown_cause(recv.as_ref());
                self.tally_ret_cause(cause);
            }
            UnkOrigin::Chain => self.stats.unk_chain += 1,
            UnkOrigin::Ivar => self.stats.unk_ivar += 1,
            UnkOrigin::Const => self.stats.unk_const += 1,
            UnkOrigin::Other => self.stats.unk_other += 1,
        }
        self.unknown_call_src
            .insert(span_of_call(call), (UnkOrigin::Chain, None));
    }

    /// Bead ita-mv5: the `project ret` cause breakdown, split out of
    /// `tally_unknown_receiver` so neither function carries both matches
    /// (software-factory's complexity ceiling — the split is the point:
    /// one function decides WHICH bucket, this one decides WHY). Called
    /// exactly once per `unk_project_ret` increment, which is what makes
    /// the six fields sum to it. `None` is the no-callee case: the
    /// receiver call's lookup never resolved a method at all.
    fn tally_ret_cause(&mut self, cause: Option<RetCause>) {
        match cause {
            Some(RetCause::Param) => self.stats.ret_param += 1,
            Some(RetCause::Ivar) => self.stats.ret_ivar += 1,
            Some(RetCause::Const) => self.stats.ret_const += 1,
            Some(RetCause::Explicit) => self.stats.ret_explicit += 1,
            Some(RetCause::Other) => self.stats.ret_other += 1,
            None => self.stats.ret_unresolved += 1,
        }
    }

    /// Bead ita-anc: `Bucket::Inconclusive`'s ancestry-blocker breakdown,
    /// tallied in the SAME call as the bucket itself — the discipline
    /// `tally_ret_cause` already established — so `anc_*`'s sum can never
    /// drift from `inconclusive`. Every `check_call` arm that reaches
    /// `Bucket::Inconclusive` calls this instead of `self.tally(Bucket::
    /// Inconclusive)` directly; `blocker` is `self.index
    /// .inconclusive_reason(c, singleton)` for the three arms whose
    /// receiver is a known project `ClassId`, `None` for the one arm
    /// that isn't (the concrete-core receiver miss).
    fn tally_inconclusive(&mut self, blocker: Option<Blocker>) {
        self.tally(Bucket::Inconclusive);
        self.tally_ancestry(blocker);
    }

    /// Bead ita-anc: split out of `tally_inconclusive` for the same
    /// reason `tally_ret_cause` is split out of `tally_unknown_receiver`
    /// (complexity ceiling — one function decides the bucket, this one
    /// decides why). Gated identically to `tally`.
    fn tally_ancestry(&mut self, blocker: Option<Blocker>) {
        if !self.census || self.silent {
            return;
        }
        let s = &mut self.stats;
        match blocker {
            Some(Blocker::Unresolved) => s.anc_unresolved += 1,
            Some(Blocker::Declared) => s.anc_declared += 1,
            Some(Blocker::Project(OpenReason::UnknownClassBodyCall)) => s.anc_dsl += 1,
            Some(Blocker::Project(
                OpenReason::ClassBodyBlock | OpenReason::BlockNestedDefinition,
            )) => s.anc_block += 1,
            Some(Blocker::Project(
                OpenReason::DynamicSuperclass
                | OpenReason::DynamicMixinReceiver
                | OpenReason::DynamicMixinArg
                | OpenReason::DynamicAttrArg
                | OpenReason::DynamicDefineMethod
                | OpenReason::DynamicAliasMethod
                | OpenReason::EvalOrSend
                | OpenReason::SingletonClassExpr
                | OpenReason::NestedDefOwner
                | OpenReason::StringSourceDefined
            )) => s.anc_meta += 1,
            Some(Blocker::Project(OpenReason::MethodMissing | OpenReason::AbstractRaise)) => {
                s.anc_missing += 1;
            }
            // Bead ita-h6l (mechanism B): a project reopening of a
            // core/stdlib/gem class — genuinely reached via `Project`
            // (unlike `DeclaredExternal` below, real project source, not
            // `merge_declared_fragment`). No allowlist closes this; the
            // ancestor is real Ruby code this checker never modeled.
            // `anc_other` bucket: same "any other reason" catch-all as
            // every non-DSL/meta/missing project open.
            Some(Blocker::Project(OpenReason::ReopenedExternal)) => s.anc_other += 1,
            // Unreachable by `inconclusive_reason`'s own contract:
            // `merge_declared_fragment` always sets a declared fragment's
            // `Blocker` to `External`, never `Project(DeclaredExternal)`.
            // Kept as a real match arm (not `unreachable!()`) so a future
            // change to that invariant fails a test, not a panic.
            Some(Blocker::Project(
                OpenReason::DeclaredExternal | OpenReason::Unattributed,
            )) => s.anc_other += 1,
            None => s.anc_na += 1,
        }
    }

    /// Bead ita-k9j (entrega 2 gate): remainder census for `anc_declared`'s
    /// `ActiveRecord::Base` slice, now that `rbi_escalate`'s own
    /// `ar_api_method` step (above) already tries every call site's exact
    /// `(id, name, singleton)` against `declarations/activerecord_api.txt`
    /// FIRST. `declared_by` is a pure function of `self.index` and `id` —
    /// the same value `rbi_escalate` already computed for this exact call
    /// — so whenever this function finds it true, `rbi_escalate` found it
    /// true too; the only way a call site still reached `Inconclusive`
    /// here is that its name matched neither `AR_API_INSTANCE` nor
    /// `AR_API_SINGLETON`. There is nothing left to classify by name: by
    /// construction every remaining site is `not_ar_api`. `blocker` is the
    /// SAME value the caller already computed for `tally_inconclusive` —
    /// this adds exactly one extra ancestor walk (`ProjectIndex::
    /// declared_by`) per call site, and only when `blocker` is `Declared`
    /// (every other blocker returns immediately, matching
    /// `tally_ancestry`'s own cost discipline). Gated identically to
    /// `tally`/`tally_ancestry`.
    fn tally_ar_base(&mut self, blocker: Option<Blocker>, id: ClassId) {
        if !self.census || self.silent {
            return;
        }
        if blocker != Some(Blocker::Declared) {
            return;
        }
        if !self.index.declared_by(id, "ActiveRecord::Base") {
            return;
        }
        self.stats.not_ar_api += 1;
    }

    /// Bead ita-xze/ita-uh1/ita-tjr/ita-k9j (entrega 2): before booking a
    /// `MethodLookup::Inconclusive` project-class receiver as blind, ask
    /// whether some RBI — or itaruby's own curated `ActiveRecord::Base`
    /// inventory — declares `name` for it. THREE populations, tried in
    /// order, getting coarser at every step:
    ///
    /// 1. `index::dsl_method_lookup` — a Tapioca DSL RBI
    ///    (`sorbet/rbi/dsl/`) reopening the RECEIVER'S OWN project class
    ///    (or a project ancestor of it) directly. This is the `dsl`
    ///    population from the ita-uh1 measurement (104.5% sig/def
    ///    coverage, vs 6.1% for `gems/`) — the bigger of the RBI-backed
    ///    levers, and the more specific: it names the exact class asking.
    /// 2. `index::rbi_method_lookup` — an EXTERNAL ancestor (unresolved
    ///    name, or a `declarations/gems.rbi` force-open stub) declares
    ///    the method instead. This is the `ancestry open` census's
    ///    single biggest population (61-86% of it at the reference
    ///    corpus) — tried second, only when (1) misses, since a class's
    ///    own DSL RBI is a more specific fact than an ancestor's.
    /// 3. `declarations/activerecord_api.txt` (`AR_API_INSTANCE`/
    ///    `AR_API_SINGLETON`) — the receiver's ancestry carries a
    ///    declared-external `ActiveRecord::Base` ancestor
    ///    (`ProjectIndex::declared_by`) and `name` is in the matching
    ///    curated set. Tried LAST: it is the coarsest of the three (a
    ///    name list, not a real signature walk), and — unlike (1)/(2) —
    ///    it needs no client `sorbet/rbi` at all, so it is the ONLY one
    ///    of the three that still fires on a project with zero Tapioca
    ///    coverage (corpus-a/corpus-b, the corpora this delivery
    ///    measured against — see `AGENTS.md`, 2026-08-22: "neither corpus-a
    ///    nor corpus-b has sorbet/rbi"). A hit resolves to `Ty::Unknown`
    ///    and NOTHING else: no `MethodDef` is synthesized, so arity is
    ///    never checked — the entire false-positive safety argument
    ///    (invariant #1, `AGENTS.md`'s ita-xze section).
    ///
    /// A hit from any of the three is CONCLUSIVE: no diagnostic either
    /// way (never was, `Inconclusive` never diagnosed before ita-xze),
    /// but carved OUT of `inconclusive`/`anc_*` into `dsl_method`,
    /// `rbi_method`, or `ar_api_method` respectively — disjoint by
    /// construction, so `total()` still sums every bucket exactly once.
    /// The caller types the call with the returned `Ty` — very often
    /// `Ty::Unknown` itself, in which case the chain dies exactly where
    /// it always did. `None` means all three missed: the caller falls
    /// through to today's exact `tally_inconclusive`/`tally_ar_base`/
    /// `note_unknown_origin` behavior, unchanged.
    ///
    /// UNLIKE `tally`/`tally_ancestry`, deliberately NOT gated on
    /// `self.census` (bead ita-uh1): before that bead, a hit's only
    /// observable effect anywhere was which `CallStats` bucket
    /// incremented, so gating the whole walk on `--stats` cost a plain
    /// `ita check` nothing. Now the effect is semantic — the returned
    /// `Ty` can keep a call chain alive — so `ita check` without
    /// `--stats` has to pay for the walk and get the win too. The
    /// `dsl_method`/`rbi_method`/`ar_api_method` buckets themselves still
    /// only count under census: `tally` (called below on a hit) keeps
    /// its own `!self.census || self.silent` gate, so `--stats`
    /// bookkeeping is unaffected. `self.silent` alone still
    /// short-circuits: this is a nested/foreign-body nested walk
    /// (go-to-def, hover, `method_return`'s cross-method inference)
    /// where paying the extra walk buys nothing this bead's contract
    /// asks for.
    ///
    /// Steps (1)/(2) are gated behind `self.rbi_map` (no client
    /// `sorbet/rbi` discovered at all — the common case for corpus-a/
    /// corpus-b) — deliberately NOT an early return for the whole
    /// function: step (3) needs no RBI and must still run, which is the
    /// entire point of shipping a curated inventory instead of only
    /// widening the RBI walk.
    fn rbi_escalate(&mut self, c: ClassId, name: &str, singleton: bool, args: &CallTypeArgs<'_>) -> Option<Ty> {
        if let Some(map) = self.rbi_map {
            if let Some(declaration) = crate::index::dsl_method_contract(self.index, c, name, singleton, map) {
                self.tally(Bucket::DslMethod);
                return Some(self.rbi_contract_call(c, name, singleton, &declaration, args));
            }
            if let Some(declaration) = crate::index::rbi_method_contract(self.index, c, name, singleton, map) {
                self.tally(Bucket::RbiMethod);
                return Some(self.rbi_contract_call(c, name, singleton, &declaration, args));
            }
        }
        if self.index.declared_by(c, "ActiveRecord::Base") {
            let hit = if singleton {
                AR_API_SINGLETON.contains(name)
            } else {
                AR_API_INSTANCE.contains(name)
            };
            if hit {
                self.tally(Bucket::ArApiMethod);
                return Some(Ty::Unknown);
            }
        }
        None
    }

    fn rbi_contract_call(
        &mut self,
        class: ClassId,
        name: &str,
        singleton: bool,
        declaration: &crate::index::RbiMethod,
        args: &CallTypeArgs<'_>,
    ) -> Ty {
        if crate::index::rbi_contract_dispatch_eligible(self.index, class, name, singleton, declaration) {
            if let (Some(sig), Some(names)) = (&declaration.definition.sorbet_sig, &declaration.definition.positional_names) {
                self.check_sorbet_args(sig, &declaration.nesting, names, &declaration.definition.keywords, name, args);
            }
        }
        declaration.return_ty(self.index)
    }

    /// Bead ita-dqo, deliverable 1 (E0107 constraint contradiction):
    /// record one usage constraint — "the local variable/parameter
    /// binding `call.receiver()` responds to `name`" — for the current
    /// method scope. Only when the receiver is a plain local variable
    /// read (the same shape `narrow_of` recognizes) and only when
    /// closed-world is on: without it E0107 can never fire (see
    /// `flush_constraints`), so there is nothing to pay for on `ita
    /// server`/LSP, which never wire `ClosedWorld` at all. Gated on
    /// `!self.silent` exactly like `tally`: a foreign method body pulled
    /// in by `method_return`/`ivar_ty`'s nested silent walk must never
    /// leak constraints into the method actually being checked.
    fn collect_constraint(&mut self, call: &CallNode<'_>, name: &str, msg_loc: (usize, usize)) {
        if self.silent || !self.closed_world() {
            return;
        }
        let Some(recv) = call.receiver() else { return };
        let Some(local) = recv.as_local_variable_read_node() else {
            return;
        };
        let binding = String::from_utf8_lossy(local.name().as_slice()).into_owned();
        self.constraints
            .entry(binding)
            .or_default()
            .push((name.to_string(), msg_loc.0, msg_loc.1));
    }

    /// Discard a binding's accumulated constraint calls on reassignment
    /// (bead ita-dqo): `x = ...`/`x ||= ...`/etc. rebind the name, so any
    /// usage constraint gathered before the write says nothing about what
    /// `x` refers to afterward.
    fn kill_constraint(&mut self, name: &str) {
        self.constraints.remove(name);
    }

    /// Project (closed-ancestry) ∪ core classes that conclusively respond
    /// to `method` — the per-distinct-method candidate set constraint
    /// contradiction proofs intersect (bead ita-dqo). Only ever computed
    /// once per distinct method name in a qualifying binding's dedup set
    /// (`flush_constraints`), never per call site: `ProjectIndex`'s
    /// reverse `methods_by_name` index bounds this to the classes that
    /// literally define `method`, not a project-wide scan (bead ita-9p9's
    /// standing lesson).
    ///
    /// ponytail: `CoreClass::Object`/`Kernel` methods (`.class`,
    /// `.inspect`, `.freeze`, ...) are deliberately excluded — every
    /// receiver responds to them, project classes included, but
    /// `methods_by_name` only tracks project classes that literally
    /// define a method themselves, not ones that merely inherit it from
    /// `Object`. Counting an Object method as a "candidate" here would
    /// make it look like only `Object` responds, manufacturing a false
    /// contradiction against any other real constraint on the same
    /// binding. Upgrade path if a fixture ever needs it: track inherited
    /// Object methods as a universal candidate that never narrows.
    fn constraint_candidates(&self, method: &str) -> Vec<String> {
        let mut names: Vec<String> = self
            .index
            .closed_candidates_for(method)
            .into_iter()
            .map(|id| self.index.class(id).path.clone())
            .collect();
        for &cc in CONSTRAINT_CORE_CLASSES {
            for &display in core_inventory_names(cc) {
                if core_inventory_has(display, method) {
                    names.push(display.to_string());
                }
            }
        }
        names.sort();
        names.dedup();
        names
    }

    /// Classify (and, when contradictory, diagnose) every binding's
    /// accumulated constraint calls at the end of the method scope that
    /// gathered them (bead ita-dqo). A no-op whenever closed-world is
    /// off (nothing was ever collected — see `collect_constraint`) or a
    /// binding never accumulated >= 2 DISTINCT method calls: a single
    /// constraint, or repeated calls to the same method, proves nothing
    /// about which of several types the binding could be.
    fn flush_constraints(&mut self) {
        if self.constraints.is_empty() {
            return;
        }
        let raw = std::mem::take(&mut self.constraints);
        for (receiver, calls) in raw {
            self.classify_binding(receiver, calls);
        }
    }

    /// One binding's verdict: dedupe its calls, resolve candidates,
    /// intersect, then either emit E0107 (empty intersection under the
    /// gates below) or hand the non-empty intersection to
    /// `intersection_outcome` for `constraint_report`.
    fn classify_binding(&mut self, receiver: String, calls: Vec<(String, usize, usize)>) {
        // Dedupe by method name, first occurrence wins. Accumulation
        // happens in program order, so the first occurrence of the
        // FIRST distinct method is always `calls[0]` itself — i.e.
        // `deduped[0]` is always the binding's earliest constraint
        // call, exactly what E0107's span and `ConstraintProof.calls`
        // ordering promise.
        let mut seen: HashSet<String> = HashSet::new();
        let mut deduped: Vec<(String, usize, usize)> = Vec::new();
        for c in calls {
            if seen.insert(c.0.clone()) {
                deduped.push(c);
            }
        }
        if deduped.len() < 2 {
            return;
        }
        let constraint_calls: Vec<ConstraintCall> = deduped
            .into_iter()
            .map(|(method, start, end)| {
                let candidates = self.constraint_candidates(&method);
                ConstraintCall {
                    method,
                    start,
                    end,
                    candidates,
                }
            })
            .collect();
        // Every call must have >= 1 candidate: an untraceable method
        // (dynamic, unknown, or reachable only through an `open`
        // class) makes the intersection uninformative, never proven
        // empty — invariant #1 (testdata/constraints scenario c).
        if constraint_calls.iter().any(|c| c.candidates.is_empty()) {
            return;
        }
        let mut intersection = constraint_calls[0].candidates.clone();
        for c in &constraint_calls[1..] {
            intersection.retain(|n| c.candidates.contains(n));
        }
        if intersection.is_empty() {
            let methods_str: Vec<String> = constraint_calls
                .iter()
                .map(|c| format!("`.{}`", c.method))
                .collect();
            let message = format!(
                "`{receiver}` is used as {}, but no type defines all of them",
                methods_str.join(", ")
            );
            let (start, end) = (constraint_calls[0].start, constraint_calls[0].end);
            let proof = ConstraintProof {
                receiver,
                calls: constraint_calls,
            };
            if !self.silent {
                self.diags.push(Diagnostic {
                    start,
                    end,
                    code: E0107_CONSTRAINT_CONTRADICTION,
                    severity: Severity::Warning,
                    message,
                    suggestion: None,
                    constraint: Some(proof.clone()),
                });
            }
            if self.collect_constraint_outcomes {
                self.constraint_outcomes
                    .push(ConstraintOutcome::Contradiction(proof));
            }
        } else if self.collect_constraint_outcomes {
            if let Some(o) = intersection_outcome(receiver, intersection, constraint_calls) {
                self.constraint_outcomes.push(o);
            }
        }
    }

    /// Conditions (a) and (b) of the closed-world contract (bead
    /// ita-2ve); (c) — the inventory miss — is checked at the call site,
    /// which already knows the allowlist missed. Every condition must
    /// hold to conclude "method does not exist" on a core receiver;
    /// anything else is v0's silence (invariant #1: false negatives are
    /// tolerated, false positives never).
    fn core_unknown_is_conclusive(&self, cc: CoreClass) -> bool {
        // (a) closed-world wired on by the binary — `ita check` proved no
        // Gemfile/Gemfile.lock/*.gemspec upward of any checked root.
        if !self.closed_world() {
            return false;
        }
        self.core_class_unpolluted(cc)
    }

    /// Condition (b) of the closed-world contract, on its own because bead
    /// ita-uye needs exactly it and none of the rest: the project never
    /// reopened `cc` nor any ancestor/mixin that could carry a method —
    /// neither as a fragment (`by_path`) nor via a metaprogramming call the
    /// walker flagged (`core_mixin`) nor through a refinement
    /// (`refined_core`/`refined_unknown`, see `index.rs`'s `refine` arm:
    /// `refine Integer do def +(o) ... end end` plus `using` makes
    /// `1 + "s"` a program MRI runs clean, and it creates no fragment)
    /// nor through an eval body nobody showed this checker
    /// (`eval_polluted_core`/`eval_polluted_unknown`, `index.rs`'s
    /// `note_opaque_eval`: `Integer.class_eval("def +(o) = 'x'")` from a
    /// method body, through a constant alias, or through a receiver that
    /// cannot be named — measured clean under MRI and accused here).
    fn core_class_unpolluted(&self, cc: CoreClass) -> bool {
        if self.index.core_mixin || self.index.refined_unknown || self.index.eval_polluted_unknown {
            return false;
        }
        !core_pollution_names(cc).iter().any(|n| {
            self.index.by_path.contains_key(*n)
                || self.index.refined_core.contains(*n)
                || self.index.eval_polluted_core.contains(*n)
        })
    }

    /// The NAME-KEYED half of condition (b), which is the only half
    /// E0108 needs: could any pollution source have defined the ONE
    /// name that would make this pairing legal at runtime?
    ///
    /// Why this is a different question from `core_class_unpolluted`
    /// above, measured rather than argued (AGENTS.md): across rails,
    /// mastodon and discourse, 248 methods are defined directly inside
    /// core-class reopenings and ZERO of them is an operator or a
    /// coercion hook. The blanket read stood E0108 down on all three for
    /// reopenings that cannot change `1 + "s"` — `Numeric#to_json_c14n`
    /// in mastodon, `Array#to_sentence` in rails.
    ///
    /// The two sides are asymmetric because MRI is (see
    /// `core_own_names` and `arg_pollution_keys` for the transcripts):
    /// the receiver's operator can only be replaced on the receiver's
    /// OWN class, while the argument's conversion hook is found anywhere
    /// in its ancestry. The sources and their readability are
    /// `index.rs`'s `PollutionSource`: a reopening's own body, a
    /// `refine` block, an `include`d project module's method set, a
    /// literal `define_method`/`alias_method`/`attr_*` injection — all
    /// readable, keyed on names; a string eval, a class-body block, a
    /// dynamic name, an unresolvable module — all `Opaque`, which still
    /// stands that ONE class down for every name; and a bare
    /// `eval(<string>)` — still every class, for every name.
    fn core_ops_unpolluted(&self, recv_cc: CoreClass, arg_cc: CoreClass, op: &str) -> bool {
        if self.index.polluted_unknown {
            return false;
        }
        let arg_keys = arg_pollution_keys(recv_cc);
        self.names_unpolluted(core_own_names(recv_cc), &[op])
            && self.names_unpolluted(core_pollution_names(arg_cc), &arg_keys)
    }

    /// Is every `key` absent from every pollution source aimed at any of
    /// `classes`? `Opaque` on one of those classes, or an unnamable
    /// target that carried one of the keys, answers no.
    fn names_unpolluted(&self, classes: &[&str], keys: &[&str]) -> bool {
        if keys.iter().any(|k| self.index.polluted_any_class.contains(*k)) {
            return false;
        }
        !classes.iter().any(|n| {
            self.index.polluted_opaque.contains(*n)
                || self
                    .index
                    .polluted_methods
                    .get(*n)
                    .is_some_and(|defined| keys.iter().any(|k| defined.contains(*k)))
        })
    }

    /// Bead ita-uye: may a receiverless call inside THIS call's block still
    /// be conclusive? Proof here is class IDENTITY: the receiver must
    /// resolve to a known, never-reopened core class, because only then do
    /// we know exactly whose method is running. The method receiving the
    /// block must also be one that merely yields
    /// (`core_block_keeps_lexical_self`) — a reopened `Array#each` could
    /// `instance_exec` the block and rebind `self`, which is exactly the
    /// DSL shape bead ita-4xy softened for.
    ///
    /// `Ty::Unknown` and project instances/classes/`Union` are all
    /// unproven, so soft: a project method (or an unresolved receiver) named
    /// `each` is exactly where a hand-rolled rebinding iterator would live,
    /// and without a known class we cannot see whether it is one.
    ///
    /// A `Ty::Unknown` arm — proving the name alone, on the argument that
    /// `Enumerable`'s published contract forbids rebinding — was built and
    /// measured, then removed by review decision: name-only evidence is
    /// argument, not proof, and invariant #1 tolerates the false negative
    /// this costs. The measurement is recorded in `AGENTS.md` rather than
    /// restated here.
    ///
    /// ponytail: a project method whose body only `yield`s keeps lexical
    /// self too, but proving it needs a per-def block-use flag in the index
    /// (`yield` only vs `&block` forwarded vs `instance_exec`). Measured
    /// surface for it on the corpora today: zero diagnostics.
    ///
    /// Asymmetry with the sibling core-conclusiveness path, named here
    /// deliberately (bead ita-tn2, raised by an external reviewer): that
    /// path also consults condition (d) — does a gem RBI reopening of the
    /// core namespace declare the name (`rbi_core_declares`) — and this
    /// guard does not. Kept that way on purpose, for two reasons. First,
    /// `rbi_core_methods` is only ever reached after
    /// `core_unknown_is_conclusive`, i.e. under `closed_world()`; wiring
    /// it in here would either gate this guard behind closed world — which
    /// is off on all three corpora, killing the capability — or trigger
    /// the one-time RBI parse inside the LSP, which never enables closed
    /// world. Second, an RBI cannot express block-rebinding semantics
    /// anyway: a reopening declares that a method exists, never that it
    /// `instance_exec`s its block, so (d) would be evidence of the wrong
    /// thing. Upgrade path if a real gem ever rebinds a core iterator:
    /// give the walker a per-RBI "reopens a core iterator" flag, cheap
    /// enough for the open-world path, and consult that here.
    fn block_keeps_lexical_self(&self, recv_ty: &Ty, name: &str) -> bool {
        let Some(cc) = core_class_of(recv_ty) else {
            return false;
        };
        core_block_keeps_lexical_self(name) && self.core_class_unpolluted(cc)
    }

    /// Condition (d) of the closed-world contract (w12 closure,
    /// Tapioca): does any gem RBI reopening of `cc`'s core namespaces —
    /// the class itself plus the `Object`/`Kernel`/... mixins its real
    /// method table runs through, same set as (b) — declare `name`?
    /// A declared method exists: silence, exactly like an inventory hit.
    /// Only ever called after `core_unknown_is_conclusive` returned true,
    /// so open-world runs never trigger the one-time RBI parse behind
    /// `rbi_core_methods`.
    fn rbi_core_declares(&self, cc: CoreClass, name: &str) -> bool {
        let methods = crate::index::rbi_core_methods(self.db);
        core_pollution_names(cc)
            .iter()
            .any(|ns| methods.get(*ns).is_some_and(|set| set.contains(name)))
    }

    /// E0108's flow-insensitive half for the scope about to be walked:
    /// the real scan on a live walk, an empty map on a `silent` one. A
    /// silent walk emits nothing (see `emit_with`), so the extra body
    /// pass there would only slow `method_return`/`ivar_ty` down.
    fn scope_operand_locals(
        &self,
        params: Option<&Node<'_>>,
        body: Option<&Node<'_>>,
    ) -> FxHashMap<String, Ty> {
        if self.silent {
            return FxHashMap::default();
        }
        prove_operand_locals(params, body)
    }

    /// E0108: the operator/operand pairings MRI raises `TypeError` on,
    /// and nothing else. Every one of these must hold or the site stays
    /// silent (invariant #1):
    ///
    /// 1. the operator is one of `+ - * /`, written with exactly one
    ///    plain positional argument and no block — a splat, a keyword
    ///    hash or a block argument is a different call shape, which is
    ///    why `pos_args` is deliberately not the input here;
    /// 2. BOTH operands are proven by `proven_operand`: a literal, or a
    ///    local whose every write in this scope is a literal of one type
    ///    AND whose flow-sensitive type at this point agrees;
    /// 3. the pairing is one MRI cannot coerce — a numeric receiver with
    ///    a `String`/`nil` operand, or `String#+` with an `Integer`/
    ///    `nil` operand. `"ab" * 2` and `1 + 2.0` are legal Ruby and
    ///    never reach the emit;
    /// 4. neither operand's core class is reopened by the project or by
    ///    any declaration (`core_class_unpolluted`, the same signal
    ///    E0101 already uses to stay silent on core receivers) — a
    ///    user-defined `coerce`, `+` or `to_str` makes the pairing legal
    ///    at runtime, and this checker never sees a body nobody showed
    ///    it. Both sides are checked, not just the receiver: `coerce`
    ///    lives on the ARGUMENT's class.
    ///
    /// Coercion through a user-defined class (`coerce`,
    /// `method_missing`) can never reach the emit at all: condition 2
    /// admits core literals only.
    fn check_operand_types(&mut self, call: &CallNode<'_>, recv_ty: &Ty, env: &Env) {
        let op_id = call.name();
        let op = op_id.as_slice();
        if !matches!(op, b"+" | b"-" | b"*" | b"/") || call.block().is_some() {
            return;
        }
        let Some(recv) = call.receiver() else { return };
        let Some(args) = call.arguments() else { return };
        let arg_list = args.arguments();
        let mut it = (&arg_list).into_iter();
        let (Some(arg), None) = (it.next(), it.next()) else {
            return;
        };
        let Some(lhs) = self.proven_operand(&recv, env) else {
            return;
        };
        let Some(rhs) = self.proven_operand(&arg, env) else {
            return;
        };
        // The flow-sensitive walk must agree with the literal proof;
        // disagreement means one of the two is not describing this
        // program point, which is not a proof.
        if *recv_ty != lhs {
            return;
        }
        let expects = match (&lhs, &rhs) {
            (Ty::Int | Ty::Float, Ty::Str | Ty::Nil) => "a numeric operand",
            (Ty::Str, Ty::Int | Ty::Nil) if op == b"+" => "a String operand",
            _ => return,
        };
        let (Some(recv_cc), Some(arg_cc)) = (core_class_of(&lhs), core_class_of(&rhs)) else {
            return;
        };
        let op_name = String::from_utf8_lossy(op);
        if !self.core_ops_unpolluted(recv_cc, arg_cc, &op_name) {
            return;
        }
        let loc = arg.location();
        let message = format!(
            "`{}` on {} expects {expects}, got {}",
            String::from_utf8_lossy(op),
            ty_name(&lhs, self.index),
            ty_name(&rhs, self.index),
        );
        self.emit(
            loc.start_offset(),
            loc.end_offset(),
            E0108_OPERAND_TYPE_MISMATCH,
            Severity::Error,
            message,
        );
    }

    /// One operand's proven type for E0108, or `None` — "not proven",
    /// i.e. silence. A literal proves itself; a local variable read is
    /// proven only when BOTH halves agree: `operand_locals` (every write
    /// to it in this scope is a literal of this type) and the walker's
    /// own flow-sensitive `env` at this program point. The env half is
    /// what makes a read BEFORE the first write unprovable; the
    /// `operand_locals` half is what makes a reassignment ANYWHERE in
    /// the scope silence an earlier site too. Anything else — a
    /// parameter, an ivar, a call, a constant — is never proven here.
    fn proven_operand(&self, node: &Node<'_>, env: &Env) -> Option<Ty> {
        if let Some(lit) = literal_operand_ty(node) {
            return Some(lit);
        }
        let local = node.as_local_variable_read_node()?;
        let name = String::from_utf8_lossy(local.name().as_slice()).into_owned();
        let proven = self.operand_locals.get(&name)?.clone();
        (env.get(&name) == Some(&proven)).then_some(proven)
    }

    fn check_arity(&mut self, m: &MethodSig, name: &str, given: usize, loc: (usize, usize)) {
        if m.arity_unknown || m.rest {
            return;
        }
        let min = m.required as usize;
        let max = min + m.optional as usize;
        if given >= min && given <= max {
            return;
        }
        let expects = if m.optional == 0 {
            format!("{min} argument{}", if min == 1 { "" } else { "s" })
        } else {
            format!("{min} to {max} arguments")
        };
        let message = format!("`{name}` expects {expects}, got {given}");
        self.emit(loc.0, loc.1, E0102_WRONG_ARITY, Severity::Error, message);
    }

    fn check_core_arity(
        &mut self,
        m: crate::core::CoreMethod,
        name: &str,
        given: usize,
        loc: (usize, usize),
    ) {
        let min = m.min_args as usize;
        let max = match m.max_args {
            Some(x) => x as usize,
            None => return,
        };
        if given >= min && given <= max {
            return;
        }
        let expects = if min == max {
            format!("{min} argument{}", if min == 1 { "" } else { "s" })
        } else {
            format!("{min} to {max} arguments")
        };
        let message = format!("`{name}` expects {expects}, got {given}");
        self.emit(loc.0, loc.1, E0102_WRONG_ARITY, Severity::Error, message);
    }

    /// E0103: positional args vs RBS sig, strict mismatches only.
    fn check_sig_args(
        &mut self,
        m: &MethodSig,
        name: &str,
        args: &CallTypeArgs<'_>,
        owner: Option<ClassId>,
        scope: &[String],
    ) {
        let Some(sig) = &m.sig else {
            if let (Some((sig, nesting)), Some(names)) = (self.effective_sorbet(m, name), &m.positional_names) {
                self.check_sorbet_args(&sig, &nesting, names, &m.keywords, name, args);
            }
            return;
        };
        let sig = sig.clone();
        let param_tys: Vec<Ty> = sig
            .params
            .iter()
            .filter_map(|p| match p {
                RbsParam::Required(t) | RbsParam::Optional(t) => Some(t),
                RbsParam::Keyword { .. } => None,
            })
            .map(|t| self.rbs_to_ty(t, owner, scope, &sig.type_params))
            .collect();
        for (i, (arg_ty, span, _)) in args.positional.iter().enumerate() {
            let Some(param_ty) = param_tys.get(i) else {
                break;
            };
            if !compatible(arg_ty, param_ty, self.index) {
                let message = format!(
                    "argument {} of `{name}` expects {}, got {}",
                    i + 1,
                    ty_name(param_ty, self.index),
                    ty_name(arg_ty, self.index),
                );
                self.emit(
                    span.0,
                    span.1,
                    E0103_ARG_TYPE_MISMATCH,
                    Severity::Error,
                    message,
                );
            }
        }
    }

    fn check_sorbet_args(
        &mut self,
        sig: &crate::sorbet_sig::SorbetSig,
        nesting: &[String],
        positional_names: &[String],
        keywords: &[(String, bool)],
        method: &str,
        args: &CallTypeArgs<'_>,
    ) {
        let Some(keyword_args) = args.keywords else { return };
        // A keyword hash can be a positional Hash in Ruby. Without a matching
        // keyword layout, do not guess which parameter received it.
        if keyword_args.iter().any(|(name, _, _)| !keywords.iter().any(|(kw, _)| kw == name)) {
            return;
        }
        for (name, (ty, span, _)) in positional_names.iter().zip(args.positional) {
            let literal_nil = args.nil_literals.contains(span);
            self.check_sorbet_argument(sig, nesting, method, name, (ty, *span, literal_nil));
        }
        for (name, ty, span) in keyword_args {
            let literal_nil = args.nil_literals.contains(span);
            self.check_sorbet_argument(sig, nesting, method, name, (ty, *span, literal_nil));
        }
    }

    fn check_sorbet_argument(
        &mut self,
        sig: &crate::sorbet_sig::SorbetSig,
        nesting: &[String],
        method: &str,
        name: &str,
        // The argument's type, its span, and whether it is WRITTEN `nil`.
        (actual, span, literal_nil): (&Ty, (usize, usize), bool),
    ) {
        let Some((_, expr)) = sig.params.iter().find(|(param, _)| param == name) else { return };
        let expected = crate::sorbet_sig::resolve_sig_ty(expr, self.index, nesting);
        if contract_accuses(actual, &expected, literal_nil, self.index) {
            self.emit(span.0, span.1, E0103_ARG_TYPE_MISMATCH, Severity::Error,
                format!("argument `{name}` of `{method}` expects {}, got {}",
                    ty_name(&expected, self.index), ty_name(actual, self.index)));
        }
    }

    /// E0106 (bead ita-yho): literal-argument-only, cast-is-provably-
    /// senseless check for attribute writers synthesized from
    /// `db/schema.rb`. Only the two cases below fire; every other literal
    /// (including a numeric string Rails' cast recovers, e.g. `"42"`) and
    /// every non-literal argument (variable, call, interpolation, `nil`,
    /// boolean) stays silent — invariant #1 extended to a permissive
    /// runtime cast, not just to `Ty::Unknown`.
    fn check_schema_cast(
        &mut self,
        col_type: &str,
        setter_name: &str,
        pos_args: &[(Ty, (usize, usize), Option<String>)],
    ) {
        let Some(risk) = cast_risk(col_type) else {
            return;
        };
        let Some((_, span, Some(lit))) = pos_args.first() else {
            return;
        };
        let senseless = match risk {
            crate::schema::CastRisk::Numeric => lit.trim().parse::<f64>().is_err(),
            crate::schema::CastRisk::Temporal => !lit.chars().any(|c| c.is_ascii_digit()),
        };
        if !senseless {
            return;
        }
        let attr = setter_name.trim_end_matches('=');
        let message = format!(
            "column `{attr}` is `{col_type}`; string literal {lit:?} cannot cast to it \
             (Rails silently drops the value instead of raising)",
        );
        self.emit(
            span.0,
            span.1,
            E0106_IMPOSSIBLE_CAST,
            Severity::Warning,
            message,
        );
    }

    /// Return type of a found project method: sig wins; otherwise infer the
    /// body (cross-file, silent, memoized, recursion-guarded).
    fn method_return(
        &mut self,
        class: ClassId,
        name: &str,
        singleton: bool,
        m: &MethodSig,
        scope: &[String],
    ) -> Ty {
        if let Some(sig) = &m.sig {
            let ret = sig.ret.clone();
            self.set_ret_cause(None);
            return self.rbs_to_ty(&ret, Some(class), scope, &sig.type_params);
        }
        if m.arity_unknown {
            self.set_ret_cause(None);
            return Ty::Unknown;
        }
        // A memo entry is only ever written after `sig_fill` answered
        // Unknown for this same key, so it is read first.
        let key = (class, name.to_string(), singleton);
        if let Some(t) = self.return_memo.get(&key) {
            let t = t.clone();
            // Bead ita-mv5: memo-hit path — the cause lives in
            // `return_cause_memo` under the SAME key, so the 2nd..Nth
            // call site of this method keeps its real cause instead of
            // losing it to a cache hit.
            let cause = if t == Ty::Unknown {
                self.return_cause_memo.get(&key).copied()
            } else {
                None
            };
            self.set_ret_cause(cause);
            return t;
        }
        let declared = self.sig_fill(m, name);
        if declared != Ty::Unknown {
            self.set_ret_cause(None);
            return declared;
        }
        if !self.in_progress.insert(key.clone()) {
            self.set_ret_cause(None);
            return Ty::Unknown;
        }
        let text = m.file.text(self.db);
        let parse = ruby_prism::parse(text.as_bytes());
        let ty = if let Some(def) = find_def_at(&parse.node(), m.def_span) {
            let was_silent = self.silent;
            self.silent = true;
            // This walks another method's body — byte offsets there
            // belong to `m.file`, not the file under the cursor, so
            // hover capture must stand down for the nested walk.
            let saved_hover = self.hover_target.take();
            // Bead ita-census: the dark census must stand down here for
            // the same reason. `dark_record` attributes every record to
            // the file under the CURSOR, and this walk's call spans
            // belong to `m.file` — without the stand-down a host file's
            // census carries foreign-span records: measured on
            // discourse, 132k of 319k rows rendered line 0 (the byte is
            // not even a char boundary in the attributed file), real
            // sites appeared as exact duplicates (the site's own walk
            // plus every host that pulled the body), and ghost sites
            // appeared in files that never call them. Family-level
            // verdicts survived (same lookup, same verdict); site-level
            // attribution — the residue audit's whole point — did not.
            let saved_dark = self.dark;
            self.dark = false;
            // Bead ita-qst: same file-mismatch guard — `cast_comments`
            // is keyed by BYTE OFFSETS, which are only meaningful
            // against the file they were collected from. Recompute
            // for `m.file`, never reuse the cursor file's ranges here
            // (a coincidental offset overlap across two unrelated
            // files could otherwise mis-cast an argument in this
            // nested walk and corrupt the return type it feeds back).
            let saved_cast_comments = std::mem::replace(
                &mut self.cast_comments,
                collect_cast_comments(
                    text,
                    parse.comments().map(|c| {
                        let loc = c.location();
                        (loc.start_offset(), loc.end_offset())
                    }),
                ),
            );
            let owner_scope = m.nesting.clone();
            let t = self.check_method_body(
                &def,
                &owner_scope,
                Some(class),
                singleton,
                m.sig.as_ref(),
            );
            self.hover_target = saved_hover;
            self.cast_comments = saved_cast_comments;
            self.silent = was_silent;
            self.dark = saved_dark;
            t
        } else {
            // ponytail: no def found at all — never ran a body walk,
            // so there is nothing more specific than Other to blame.
            self.set_ret_cause(Some(RetCause::Other));
            Ty::Unknown
        };
        self.in_progress.remove(&key);
        // Bead ita-mv5: freshly-computed path — pair `return_cause_memo`
        // with `return_memo` right here, at the exact key both are keyed
        // by. `check_method_body` already set `self.last_ret_cause` (or
        // the `None => ...` arm above did) for the walk that just
        // produced `ty`.
        self.memoize_ret_cause(&key, &ty);
        self.return_memo.insert(key, ty.clone());
        ty
    }

    /// Consumer types use the explicit contract; the source body is checked
    /// independently by `check_method_body`, never against this assumed result.
    fn sig_fill(&self, m: &MethodSig, name: &str) -> Ty {
        note_contract_work(|w| w.return_probes += 1);
        self.effective_sorbet(m, name)
            .filter(|(sig, _)| !sig.void)
            .and_then(|(sig, nesting)| sig.ret.map(|expr| {
                crate::sorbet_sig::resolve_sig_ty(&expr, self.index, &nesting)
            }))
            .unwrap_or(Ty::Unknown)
    }

    /// Bead ita-mv5: `last_ret_cause` writer. Exists so `method_return`'s
    /// five exits each cost one call instead of one `if self.census`
    /// branch — software-factory's complexity ceiling counts those, and
    /// the ceiling was right: the bookkeeping is one concern, not five.
    fn set_ret_cause(&mut self, cause: Option<RetCause>) {
        if self.census {
            self.last_ret_cause = cause;
        }
    }

    /// Bead ita-mv5: the freshly-computed path's half of the pairing —
    /// `return_cause_memo` written at the SAME key `return_memo` is about
    /// to be written at, so a later cache hit can recover the cause.
    /// `check_method_body` (or `method_return`'s no-def arm) already left
    /// the cause in `last_ret_cause` for the walk that produced `ty`.
    fn memoize_ret_cause(&mut self, key: &(ClassId, String, bool), ty: &Ty) {
        if !self.census {
            return;
        }
        if *ty == Ty::Unknown {
            let cause = self.last_ret_cause.unwrap_or(RetCause::Other);
            self.return_cause_memo.insert(key.clone(), cause);
            self.last_ret_cause = Some(cause);
        } else {
            self.last_ret_cause = None;
        }
    }

    /// Instance-variable type (bead ita-u1t, single-walk-per-class perf
    /// fix bead ita-9p9): the union of every `@name = <expr>` assignment
    /// across the class's own instance methods — walked via
    /// `check_method_body` (silent, recursion guarded: the exact reuse
    /// pattern `method_return` already establishes for cross-method
    /// inference), plus an `ivar_capture` side channel that intercepts
    /// every `InstanceVariableWriteNode` on this class, no matter how
    /// deeply nested inside the body, bucketed by ivar name. The whole
    /// class is walked exactly ONCE (memoized per `ClassId`, not per
    /// `(ClassId, name)`): before this bead, each distinct ivar name ever
    /// read on a class re-walked every instance method from scratch,
    /// O(distinct ivars × methods) on classes with hundreds of methods and
    /// dozens of ivars (15x wall-clock regression on corpus-c,
    /// bead ita-9p9). Deliberately NOT `Ty::union` per name: two different
    /// assignment types collapse straight to `Ty::Unknown`, never
    /// `Ty::Union` — `compatible()` treats a `Union` argument as "every
    /// member must match its param", stricter than Unknown's "always
    /// passes", so a real union here could still trigger a false-positive
    /// E0103. No visible assignment anywhere: Unknown, never an error
    /// (invariant #1). Only instance methods are walked — a class-level
    /// `@x` inside `def self.foo` is a different storage slot in real
    /// Ruby, not the same variable. Ivar narrowing itself stays out of
    /// scope (contract): only the type is inferred here, never
    /// flow-refined. Recursion guard is per-class now (was per-name): a
    /// nested read of this same class's ivars mid-walk — self-referential
    /// or cross-ivar — falls back to `Ty::Unknown`, same fixpoint fallback
    /// `method_return` uses for method recursion; safe under invariant #1
    /// (Unknown never manufactures a diagnostic, only a possible false
    /// negative), confirmed measured on corpus-c: identical error hashes (2)
    /// and warning ceiling (4321) before and after this bead.
    ///
    /// The fold only sees `@name` writes in `class`'s own instance
    /// methods, so it is a proof only when no other writer exists:
    /// `ivar_writes_hidden` turns every other writer path into Unknown.
    fn ivar_ty(&mut self, class: ClassId, name: &str) -> Ty {
        if !self.ensure_ivar_walk(class) {
            return Ty::Unknown; // recursive read mid-computation: fixpoint fallback
        }
        let own = self.ivar_class_memo.get(&class).and_then(|map| map.get(name)).cloned();
        match own {
            Some(ty) if ty != Ty::Unknown && !self.ivar_writes_hidden(class, name) => ty,
            _ => Ty::Unknown,
        }
    }

    /// Walks `class`'s instance methods once and memoizes the folded
    /// writes; `false` while that walk is already in progress.
    fn ensure_ivar_walk(&mut self, class: ClassId) -> bool {
        if self.ivar_class_memo.contains_key(&class) {
            return true;
        }
        if !self.ivar_class_in_progress.insert(class) {
            return false;
        }
        let collected = self.walk_class_ivars(class);
        self.ivar_class_in_progress.remove(&class);
        self.ivar_class_memo.insert(class, fold_ivar_writes(collected));
        true
    }

    /// Can `@name` on an instance of `class` be written by anything the
    /// per-class walk did not see as a typed write? Every answer this
    /// cannot rule out is `true` (invariant #1: an ivar whose only
    /// visible write is `@x = nil` must not read back as exactly nil
    /// when an attribute writer or reflection can replace it):
    ///
    /// - a write the index saw outside any class's instance method, in a
    ///   block that may rebind `self`, or through reflection
    ///   (`ProjectIndex::hidden_ivar_writes`);
    /// - a writer method `name=` (`attr_writer`/`attr_accessor`, a
    ///   `define_method`, a hand-written one) anywhere in the ancestry or
    ///   the descendants, or an ancestry that cannot rule one out;
    /// - a write to the same name in any ancestor's or descendant's own
    ///   instance methods, which run on this same object.
    fn ivar_writes_hidden(&mut self, class: ClassId, name: &str) -> bool {
        if self.index.hidden_ivar_writes_any || self.index.hidden_ivar_writes.contains(name) {
            return true;
        }
        let writer = format!("{}=", name.trim_start_matches('@'));
        if !matches!(self.index.lookup_method(class, &writer), MethodLookup::NotFound)
            || self.index.descendant_defines(class, &writer, false)
        {
            return true;
        }
        let (ancestors, _) = self.index.ancestors(class);
        let family: Vec<ClassId> =
            ancestors.into_iter().chain(descendants_of(self.index, class)).filter(|&c| c != class).collect();
        family.into_iter().any(|member| {
            !self.ensure_ivar_walk(member)
                || self.ivar_class_memo.get(&member).is_some_and(|map| map.contains_key(name))
        })
    }

    /// Side channel for `ivar_ty`'s silent re-walk: record one write's type
    /// when it lands on the (class, name) being collected.
    fn capture_ivar_write(&mut self, self_ty: SelfTy, name: &[u8], ty: Ty) {
        let SelfTy::Instance(c) = self_ty else { return };
        if let Some((cap_class, values)) = self.ivar_capture.as_mut() {
            if *cap_class == c {
                values.entry(String::from_utf8_lossy(name).into_owned()).or_default().push(ty);
            }
        }
    }

    /// Single silent walk of every instance method of `class`, capturing
    /// every `@name = expr` write into a per-name bucket (bead ita-9p9).
    /// Source files are parsed exactly once each (grouped by `SourceFile`),
    /// not once per method — a class with hundreds of methods living in
    /// one giant model file used to re-parse that whole file per method.
    fn walk_class_ivars(&mut self, class: ClassId) -> HashMap<String, Vec<Ty>> {
        let methods: Vec<MethodSig> = self.index.class(class).methods.values().cloned().collect();
        let owner_scope = self.index.class(class).nesting.clone();
        let was_silent = self.silent;
        self.silent = true;
        // Same file-mismatch guard as `method_return`: these bodies are
        // not the cursor's file, hover capture stands down.
        let saved_hover = self.hover_target.take();
        let saved_capture = self.ivar_capture.replace((class, HashMap::new()));
        // Bead ita-qst: `cast_comments` is byte-offset-keyed, so it must
        // be recomputed per `file` below (same reasoning as
        // `method_return`'s cross-file guard) — saved once here and
        // restored once after the whole walk, since every iteration
        // overwrites it for its own file in turn.
        let saved_cast_comments = std::mem::take(&mut self.cast_comments);
        let mut by_file: HashMap<SourceFile, Vec<&MethodSig>> = HashMap::new();
        for m in &methods {
            by_file.entry(m.file).or_default().push(m);
        }
        for (file, ms) in &by_file {
            let text = file.text(self.db);
            let parse = ruby_prism::parse(text.as_bytes());
            self.cast_comments = collect_cast_comments(
                text,
                parse.comments().map(|c| {
                    let loc = c.location();
                    (loc.start_offset(), loc.end_offset())
                }),
            );
            for m in ms {
                if let Some(def) = find_def_at(&parse.node(), m.def_span) {
                    self.check_method_body(&def, &owner_scope, Some(class), false, m.sig.as_ref());
                }
            }
        }
        let collected = self.ivar_capture.take().map_or(HashMap::new(), |(_, m)| m);
        self.ivar_capture = saved_capture;
        self.hover_target = saved_hover;
        self.cast_comments = saved_cast_comments;
        self.silent = was_silent;
        collected
    }

    /// Bead ita-s12: `type_params` is the enclosing sig's OWN
    /// `[T, U, ...]` list (empty for a sig with none). A `RbsTy::Simple`
    /// name matching one of them binds to `Ty::Unknown` before any other
    /// interpretation is tried — a generic method's type variable is
    /// never a real project/core class, no matter what `resolve_const`
    /// would otherwise find (measured: ruby-lsp vendors an actual `T`
    /// module, so `#: [T] (String, T) -> T` used to type its own type
    /// variable as that unrelated class and reject any non-`T` argument).
    /// Sound by construction either way (invariant #1): whether the name
    /// resolves to a real class or not, the fallback was already
    /// `Ty::Unknown` — this only changes WHICH names take that fallback.
    fn rbs_to_ty(&self, t: &RbsTy, self_class: Option<ClassId>, scope: &[String], type_params: &[String]) -> Ty {
        match t {
            RbsTy::Simple(name) if type_params.iter().any(|p| p == name) => Ty::Unknown,
            RbsTy::Simple(name) => match name.as_str() {
                "Integer" => Ty::Int,
                "Float" => Ty::Float,
                "String" => Ty::Str,
                "Symbol" => Ty::Sym,
                "bool" | "true" | "false" | "TrueClass" | "FalseClass" => Ty::Bool,
                "nil" | "NilClass" => Ty::Nil,
                "void" | "untyped" | "top" | "bot" => Ty::Unknown,
                "self" => self_class.map_or(Ty::Unknown, Ty::Instance),
                other => self
                    .index
                    .resolve_const(scope, other)
                    .map_or(Ty::Unknown, Ty::Instance),
            },
            RbsTy::Generic(name, args) => match (name.as_str(), args.as_slice()) {
                ("Array", [e]) => Ty::Array(Box::new(self.rbs_to_ty(e, self_class, scope, type_params))),
                ("Hash", [k, v]) => Ty::Hash(
                    Box::new(self.rbs_to_ty(k, self_class, scope, type_params)),
                    Box::new(self.rbs_to_ty(v, self_class, scope, type_params)),
                ),
                _ => Ty::Unknown,
            },
            RbsTy::Nilable(inner) => Ty::union(self.rbs_to_ty(inner, self_class, scope, type_params), Ty::Nil),
            RbsTy::Union(parts) => parts
                .iter()
                .map(|p| self.rbs_to_ty(p, self_class, scope, type_params))
                .reduce(Ty::union)
                .unwrap_or(Ty::Unknown),
        }
    }
}

/// Every transitive subclass of `id` (the runtime classes an instance
/// method of `id` can run on).
fn descendants_of(index: &ProjectIndex, id: ClassId) -> Vec<ClassId> {
    let mut out = Vec::new();
    let mut stack = vec![id];
    while let Some(cur) = stack.pop() {
        for &kid in index.subclasses.get(&cur).into_iter().flatten() {
            if !out.contains(&kid) {
                out.push(kid);
                stack.push(kid);
            }
        }
    }
    out
}

/// Fold each ivar's collected assignment types (from `Checker::walk_class_ivars`)
/// into one `Ty` per name: two assignments of the same type keep that type;
/// a differing type, or any `Unknown`, collapses straight to `Ty::Unknown` —
/// deliberately never `Ty::Union` (see `Checker::ivar_ty`'s doc comment for
/// why). A name with no captured writes never gets an entry here; callers
/// treat that absence as `Ty::Unknown` too.
fn fold_ivar_writes(collected: HashMap<String, Vec<Ty>>) -> HashMap<String, Ty> {
    collected
        .into_iter()
        .map(|(name, values)| {
            let mut result: Option<Ty> = None;
            for t in values {
                result = Some(match result {
                    None => t,
                    Some(prev) if prev == t => prev,
                    Some(_) => Ty::Unknown,
                });
            }
            (name, result.unwrap_or(Ty::Unknown))
        })
        .collect()
}

/// Non-empty constraint intersection → the outcome `constraint_report`
/// names for it, if any (bead ita-dqo): exactly 1 candidate = `Inferred`,
/// 2..=4 = `UnionCandidate`. ponytail: intersections of > 4 candidates
/// aren't classified — the contract only names Inferred (1),
/// `UnionCandidate` (2..=4), and Contradiction (0); no fixture needs a
/// fourth bucket for a wider union.
fn intersection_outcome(
    receiver: String,
    mut intersection: Vec<String>,
    calls: Vec<ConstraintCall>,
) -> Option<ConstraintOutcome> {
    if intersection.len() == 1 {
        let ty = intersection.pop()?;
        return Some(ConstraintOutcome::Inferred {
            receiver,
            ty,
            calls,
        });
    }
    if intersection.len() <= 4 {
        return Some(ConstraintOutcome::UnionCandidate {
            receiver,
            candidates: intersection,
            calls,
        });
    }
    None
}

fn self_ty_of(class: Option<ClassId>, singleton: bool) -> SelfTy {
    match class {
        Some(c) if singleton => SelfTy::Class(c),
        Some(c) => SelfTy::Instance(c),
        None => SelfTy::Unknown,
    }
}

/// Strict-but-safe compatibility: Unknown always passes. An `Instance` arg
/// also passes when the param class is in its ancestry (a subclass IS the
/// param type at runtime) — or when that ancestry is incomplete, because
/// then non-subtyping is unprovable and invariant #1 ranks silence above
/// accusation. Surfaced by bead ita-p24: once malformed sig comments stopped
/// dying as E0105, genuine subtype calls (e.g. `Entry::Method` into
/// `(entry: Entry)`) fired E0103 on exact-equality.
fn compatible(arg: &Ty, param: &Ty, index: &ProjectIndex) -> bool {
    match (arg, param) {
        (Ty::Unknown, _) | (_, Ty::Unknown) => true,
        // An instance of a MODULE (`self` in a module method, a value typed
        // by a module name) is an instance of some class that includes it,
        // which this checker cannot name: never proof against anything.
        (Ty::Instance(a), _) if index.class(*a).is_module => true,
        (Ty::Union(parts), p) => parts.iter().all(|a| compatible(a, p, index)),
        (a, Ty::Union(parts)) => parts.iter().any(|p| compatible(a, p, index)),
        (Ty::Array(a), Ty::Array(p)) => compatible(a, p, index),
        (Ty::Hash(ak, av), Ty::Hash(pk, pv)) => {
            compatible(ak, pk, index) && compatible(av, pv, index)
        }
        (Ty::Instance(a), Ty::Instance(p)) => {
            if a == p {
                return true;
            }
            let (anc, complete) = index.ancestors(*a);
            anc.contains(p) || !complete
        }
        (a, p) => a == p,
    }
}

/// Sorbet contract conformance (E0103/E0109) under invariant #1. Nil
/// membership is a FLOW fact this checker cannot prove: a `T.nilable`
/// param seeds `T | nil` into the body and nothing strips nil through
/// `x || d`, `x ||= d` or guards such as `return if x.blank?`, and a bare
/// `nil` read back from a local, an ivar or a call may have been replaced
/// by a write it never saw (an attribute writer, reflection). So every
/// `nil` inside `actual` counts as Unknown — the rest of a union still
/// answers for itself — unless the caller proved the value is a `nil`
/// written right there (`literal_nil`), which keeps a top-level `Nil`.
fn contract_accuses(actual: &Ty, expected: &Ty, literal_nil: bool, index: &ProjectIndex) -> bool {
    fn unprove_nil(t: &Ty) -> Ty {
        match t {
            Ty::Nil => Ty::Unknown,
            Ty::Union(parts) => Ty::Union(parts.iter().map(unprove_nil).collect()),
            Ty::Array(e) => Ty::Array(Box::new(unprove_nil(e))),
            Ty::Hash(k, v) => Ty::Hash(Box::new(unprove_nil(k)), Box::new(unprove_nil(v))),
            other => other.clone(),
        }
    }
    let proven = if literal_nil && *actual == Ty::Nil { Ty::Nil } else { unprove_nil(actual) };
    !compatible(&proven, expected, index)
}

/// Short type rendering shared by diagnostics, hover, and the CLI: `nil`,
/// `Integer`, `String`, project class paths, `Foo | nil` unions.
pub fn ty_name(t: &Ty, index: &ProjectIndex) -> String {
    match t {
        Ty::Unknown => "untyped".to_string(),
        Ty::Nil => "nil".to_string(),
        Ty::Bool => "bool".to_string(),
        Ty::Int => "Integer".to_string(),
        Ty::Float => "Float".to_string(),
        Ty::Str => "String".to_string(),
        Ty::Sym => "Symbol".to_string(),
        Ty::Instance(c) => index.class(*c).path.clone(),
        Ty::Class(c) => format!("Class[{}]", index.class(*c).path),
        Ty::Array(e) => format!("Array[{}]", ty_name(e, index)),
        Ty::Hash(k, v) => format!("Hash[{}, {}]", ty_name(k, index), ty_name(v, index)),
        Ty::Union(parts) => parts
            .iter()
            .map(|p| ty_name(p, index))
            .collect::<Vec<_>>()
            .join(" | "),
    }
}

/// Hover's arity summary for a resolved method: `"2"`, `"1..2"` with
/// optional params, `"0+"` with rest; `"unknown"` when the def's arity
/// cannot be known.
fn arity_str(m: &MethodSig) -> String {
    if m.arity_unknown {
        return "unknown".to_string();
    }
    let rest = if m.rest { "+" } else { "" };
    if m.optional == 0 {
        format!("{}{}", m.required, rest)
    } else {
        format!("{}..{}{}", m.required, m.required + m.optional, rest)
    }
}

/// What a core call site proves about its arguments: `args` is the
/// positional argument types when the call shape is exact (no splat, no
/// keyword hash, no `...`), and `int_literal` the value of a first
/// argument written as a plain Integer literal.
struct CoreCallShape<'a> {
    args: Option<&'a [Ty]>,
    int_literal: Option<i64>,
}

/// The first positional argument's value when it is written as an
/// Integer literal (`2`, `-1`), else `None`.
fn first_int_literal(call: &CallNode<'_>) -> Option<i64> {
    let args = call.arguments()?;
    let first = args.arguments().iter().next()?;
    let text = first.as_integer_node()?.location().as_slice().to_vec();
    String::from_utf8(text).ok()?.replace('_', "").parse().ok()
}

/// A core method's return at THIS call site. The table's `CoreRet` is
/// the answer for the argument-free form (or an argument that cannot
/// change the type); every method below returns a different type
/// depending on its arguments, so it answers only when the argument
/// types make the result trivially provable and is `Ty::Unknown`
/// otherwise (invariant #1: a wrong precise type here becomes an E0101,
/// E0103 or E0109 accusation downstream).
fn core_call_ret(cc: CoreClass, name: &str, ret: CoreRet, recv: &Ty, shape: &CoreCallShape<'_>) -> Ty {
    let Some(args) = shape.args else {
        return if core_ret_depends_on_args(cc, name) { Ty::Unknown } else { core_ret_to_ty(ret, recv) };
    };
    let answer = match cc {
        CoreClass::Integer => integer_call_ret(name, args, shape.int_literal),
        CoreClass::Float => float_call_ret(name, args, shape.int_literal),
        CoreClass::Array => array_call_ret(name, args, recv),
        _ => None,
    };
    answer.unwrap_or_else(|| core_ret_to_ty(ret, recv))
}

/// `Integer` methods whose return follows the arguments; `None` means
/// the table's own return applies.
fn integer_call_ret(name: &str, args: &[Ty], int_literal: Option<i64>) -> Option<Ty> {
    let ty = match name {
        // Arithmetic takes the operand's numeric class (`1 * 1.5` is a
        // Float); anything else (Rational, BigDecimal, an unknown object's
        // `coerce`) is unproven.
        "+" | "-" | "*" | "/" | "%" => match args {
            [Ty::Int] => Ty::Int,
            [Ty::Float] => Ty::Float,
            _ => Ty::Unknown,
        },
        // A negative exponent answers a Rational: only a literal
        // non-negative one is proven Integer.
        "**" => match (args, int_literal) {
            ([Ty::Int], Some(n)) if n >= 0 => Ty::Int,
            _ => Ty::Unknown,
        },
        // `clamp` returns the receiver or one of its bounds.
        "clamp" => match args {
            [Ty::Int, Ty::Int] => Ty::Int,
            _ => Ty::Unknown,
        },
        _ => return None,
    };
    Some(ty)
}

/// `Float` methods whose return follows the arguments; `None` means the
/// table's own return applies.
fn float_call_ret(name: &str, args: &[Ty], int_literal: Option<i64>) -> Option<Ty> {
    let ty = match name {
        // An Integer or Float operand keeps a Float; a BigDecimal or
        // Complex one does not.
        "+" | "-" | "*" | "/" | "%" => match args {
            [Ty::Int | Ty::Float] => Ty::Float,
            _ => Ty::Unknown,
        },
        // `(-8.0) ** 0.5` is a Complex: only an Integer exponent is proven.
        "**" => match args {
            [Ty::Int] => Ty::Float,
            _ => Ty::Unknown,
        },
        "round" | "floor" | "ceil" => float_digits_ret(args, int_literal),
        _ => return None,
    };
    Some(ty)
}

/// `Float#round`/`#floor`/`#ceil`: no digits (or digits <= 0) answer an
/// Integer, positive digits a Float; digits nobody wrote are unproven.
fn float_digits_ret(args: &[Ty], int_literal: Option<i64>) -> Ty {
    match (args, int_literal) {
        ([], _) => Ty::Int,
        ([Ty::Int], Some(n)) if n > 0 => Ty::Float,
        ([Ty::Int], Some(_)) => Ty::Int,
        _ => Ty::Unknown,
    }
}

/// `Array` methods whose return follows the arguments; `None` means the
/// table's own return applies.
fn array_call_ret(name: &str, args: &[Ty], recv: &Ty) -> Option<Ty> {
    match name {
        // With a count, these return an Array of elements, not one.
        "first" | "last" | "min" | "max" | "pop" | "shift" => match (args, recv) {
            ([], _) => None,
            ([Ty::Int], Ty::Array(_)) => Some(recv.clone()),
            _ => Some(Ty::Unknown),
        },
        "flatten" => Some(flatten_ret(recv, args.is_empty())),
        _ => None,
    }
}

/// Does `core_call_ret` read the arguments for this method? Those answer
/// `Unknown` when the call shape hides the arguments (a splat, a keyword
/// hash, `...`).
fn core_ret_depends_on_args(cc: CoreClass, name: &str) -> bool {
    match cc {
        CoreClass::Integer => matches!(name, "+" | "-" | "*" | "/" | "%" | "**" | "clamp"),
        CoreClass::Float => matches!(name, "+" | "-" | "*" | "/" | "%" | "**" | "round" | "floor" | "ceil"),
        CoreClass::Array => {
            matches!(name, "first" | "last" | "min" | "max" | "pop" | "shift" | "flatten")
        }
        _ => false,
    }
}

/// `Array#flatten`: the receiver's type only when its elements cannot
/// be flattened at all (a core scalar), the innermost element type for
/// a full flatten of nested arrays of scalars; anything that may respond
/// to `to_ary` (a project instance, an unknown, a union) is unproven.
fn flatten_ret(recv: &Ty, full: bool) -> Ty {
    fn scalar(t: &Ty) -> bool {
        matches!(t, Ty::Int | Ty::Float | Ty::Str | Ty::Sym | Ty::Bool | Ty::Nil | Ty::Hash(_, _))
    }
    let Ty::Array(elem) = recv else { return Ty::Unknown };
    if scalar(elem) {
        return recv.clone();
    }
    if !full {
        return Ty::Unknown;
    }
    let mut inner: &Ty = elem;
    while let Ty::Array(e) = inner {
        inner = e;
    }
    if scalar(inner) { Ty::Array(Box::new(inner.clone())) } else { Ty::Unknown }
}

fn core_ret_to_ty(ret: CoreRet, recv: &Ty) -> Ty {
    match ret {
        CoreRet::Int => Ty::Int,
        CoreRet::Float => Ty::Float,
        CoreRet::Str => Ty::Str,
        CoreRet::Sym => Ty::Sym,
        CoreRet::Bool => Ty::Bool,
        CoreRet::Nil => Ty::Nil,
        CoreRet::SelfSame => recv.clone(),
        CoreRet::Elem => match recv {
            Ty::Array(e) => (**e).clone(),
            _ => Ty::Unknown,
        },
        CoreRet::KeyArray => match recv {
            Ty::Hash(k, _) => Ty::Array(k.clone()),
            _ => Ty::Unknown,
        },
        CoreRet::ValArray => match recv {
            Ty::Hash(_, v) => Ty::Array(v.clone()),
            _ => Ty::Unknown,
        },
        CoreRet::StrArray => Ty::Array(Box::new(Ty::Str)),
        CoreRet::Unknown => Ty::Unknown,
    }
}

/// Bead ita-r8k: is `predicate` directly a `defined?(X)` call whose
/// argument is a literal constant path? Returns `X`'s exact path text
/// (`const_path_str`'s format, e.g. `::AppBuilder`) when so — the same
/// string `check_const_ref` compares against, since it is used as
/// written, never re-resolved. Only this direct-predicate shape is in
/// scope (measured sites, bead ita-r8k): `defined?(X) && y` or
/// `defined?(X) || y` are combinator predicates, not a bare `DefinedNode`,
/// so `predicate.as_defined_node()` misses them and this returns `None` —
/// a real gap (no suppression) rather than a wrong one, consistent with
/// invariant #1 (silence-by-omission is always safe; the follow-up is
/// adding the combinator shapes once a corpus site measures one).
fn defined_guard_const(predicate: &Node<'_>) -> Option<String> {
    let defined = predicate.as_defined_node()?;
    const_path_str(&defined.value())
}

/// Bead ita-w2c: the spans of the calls that are the DIRECT statements
/// of a block body — the sole subject of a raise assertion, never a call
/// nested inside one. `expect { [X] }.to raise_error` records nothing,
/// so `X` keeps firing: that is the owner's narrow scope made
/// mechanical, and the control that proves it.
fn direct_statement_call_spans(body: &Node<'_>) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    if let Some(stmts) = body.as_statements_node() {
        for s in &stmts.body() {
            if let Some(c) = s.as_call_node() {
                out.push(span_of_call(&c));
            }
        }
    } else if let Some(c) = body.as_call_node() {
        out.push(span_of_call(&c));
    }
    out
}

/// Bead ita-w2c, the `RSpec` half: `expect { <subject> }.to raise_error(...)`
/// (and `.to_not`) — the subject is the `expect` call's block body, which
/// this call's own receiver inference walks. `raise_error` must be the
/// first argument of the `.to`; `expect` itself must be receiverless,
/// the only spelling the measured corpus site and `RSpec`'s DSL use.
fn rspec_raise_subject_spans(call: &CallNode<'_>) -> Vec<(usize, usize)> {
    let name = call.name();
    if name.as_slice() != b"to" && name.as_slice() != b"to_not" {
        return Vec::new();
    }
    let matcher = call
        .arguments()
        .and_then(|a| a.arguments().iter().next())
        .and_then(|m| m.as_call_node());
    let Some(matcher) = matcher else { return Vec::new() };
    if matcher.name().as_slice() != b"raise_error" {
        return Vec::new();
    }
    let Some(expect) = call.receiver().and_then(|r| r.as_call_node()) else {
        return Vec::new();
    };
    if expect.receiver().is_some() || expect.name().as_slice() != b"expect" {
        return Vec::new();
    }
    let Some(body) = expect.block().and_then(|b| b.as_block_node()).and_then(|b| b.body()) else {
        return Vec::new();
    };
    direct_statement_call_spans(&body)
}

/// Bead ita-w2c: the name `Checker::narrowed_names` can key an
/// expression on — a local variable read (`x`), or a receiverless,
/// argument-less, block-less call (`rack_app`), which is how a
/// predicate like `rack_app.is_a?(Class)` names something that is not a
/// local at all. Everything else (an ivar, an explicit-receiver call, a
/// literal, a call with arguments) has no identity a second occurrence
/// could be matched against, so it gets no key and no fact.
fn expr_name(node: &Node<'_>) -> Option<String> {
    if let Some(local) = node.as_local_variable_read_node() {
        return Some(String::from_utf8_lossy(local.name().as_slice()).into_owned());
    }
    let call = node.as_call_node()?;
    (call.receiver().is_none() && call.arguments().is_none() && call.block().is_none())
        .then(|| String::from_utf8_lossy(call.name().as_slice()).into_owned())
}

/// Bead ita-w2c: `respond_to?(:m)` / `respond_to?(:m, true)` (and the
/// `false` mirror) as a bare predicate — the true branch only runs when
/// the receiver really answers `m`, so a receiverless call to `m` inside
/// it cannot be a `NoMethodError`. Returns `m`.
///
/// Only the DIRECT predicate shape counts, same reasoning (and the same
/// measured-site argument) as `defined_guard_const` above: `respond_to?(:m)
/// && y` is a combinator, not a bare `CallNode` predicate, so it returns
/// `None` — a gap (no suppression), never a wrong one (invariant #1).
///
/// The receiver must be implicit: an explicit receiver would make the
/// guard a statement about THAT object, while the diagnostic this feeds
/// is a self-send. The second argument, when present, must be a literal
/// boolean (`respond_to?(:m, true)` also counts private methods, which
/// still answers "the method exists"); anything else is not the shape
/// this guard has evidence for.
fn respond_to_guard_name(predicate: &Node<'_>) -> Option<String> {
    let call = predicate.as_call_node()?;
    if call.receiver().is_some() || call.name().as_slice() != b"respond_to?" {
        return None;
    }
    let args = call.arguments()?;
    let mut it = args.arguments().iter();
    let name = String::from_utf8_lossy(it.next()?.as_symbol_node()?.unescaped()).into_owned();
    if let Some(second) = it.next() {
        if it.next().is_some() {
            return None;
        }
        if second.as_true_node().is_none() && second.as_false_node().is_none() {
            return None;
        }
    }
    Some(name)
}

/// Branch join: for every var present in any branch env, the merged value is
/// the union across branches (vars missing from a branch keep the pre-branch
/// value, or Nil when new).
fn join_envs(base: &mut Env, branches: &[Env]) {
    let mut keys: HashSet<String> = HashSet::new();
    for b in branches {
        keys.extend(b.keys().cloned());
    }
    for key in keys {
        let mut ty: Option<Ty> = None;
        for b in branches {
            let t = b
                .get(&key)
                .cloned()
                .or_else(|| base.get(&key).cloned())
                .unwrap_or(Ty::Nil);
            ty = Some(match ty {
                Some(prev) => Ty::union(prev, t),
                None => t,
            });
        }
        if let Some(t) = ty {
            base.insert(key, t);
        }
    }
}

/// Flow narrowing (bead ita-u1t): apply a `Narrow` fact to the branch
/// taken when its predicate is TRUE.
fn apply_narrow_true(env: &mut Env, nw: &Narrow) {
    match &nw.kind {
        NarrowKind::IsA(t) => {
            env.insert(nw.var.clone(), t.clone());
        }
        NarrowKind::NilCheck => {
            env.insert(nw.var.clone(), Ty::Nil);
        }
        // `if x` truthy: `x` is neither `nil` nor `false` here (bead
        // ita-w9i) — same math as `NilCheck`'s false-branch strip.
        NarrowKind::Truthy => {
            let cur = env.get(&nw.var).cloned().unwrap_or(Ty::Unknown);
            env.insert(nw.var.clone(), strip_nil(cur));
        }
        // `a.nil? || b` being true proves nothing about `a` specifically
        // (bead ita-w9i) — deliberately a no-op, see `NarrowKind` doc.
        NarrowKind::NilCheckOrGuard => {}
    }
}

/// Apply a `Narrow` fact to the branch taken when its predicate is FALSE.
/// `base` is the env *before* the branch, for `nil?`'s "strip Nil out of
/// whatever was already known". `is_a?`'s false branch (bead ita-zdy):
/// when the receiver's current type is a CLOSED union (every member
/// concrete — see `eliminate_union_member`'s doc comment for exactly
/// what that means and why), failing `x.is_a?(Foo)` proves `Foo` is not
/// the runtime type, so `Foo` can be dropped from the union — Sorbet
/// does this elimination too. Anything else (not a union, `Foo` isn't a
/// member, or the receiver isn't even narrowed) is the original v0 "no
/// else info" no-op: `env` is already a clone of `base`, so leaving it
/// alone is correct by construction. `Truthy`'s false branch (bead
/// ita-w9i) is a no-op for a different reason: Ruby's falsy set is
/// `nil | false` and this fact alone can't tell which one held, so
/// narrowing to `Nil` would risk manufacturing a diagnostic on an actual
/// `false` value (invariant #1).
fn apply_narrow_false(env: &mut Env, nw: &Narrow, base: &Env) {
    match &nw.kind {
        NarrowKind::NilCheck | NarrowKind::NilCheckOrGuard => {
            let cur = base.get(&nw.var).cloned().unwrap_or(Ty::Unknown);
            env.insert(nw.var.clone(), strip_nil(cur));
        }
        NarrowKind::IsA(tested) => {
            let cur = base.get(&nw.var).cloned().unwrap_or(Ty::Unknown);
            if let Some(narrowed) = eliminate_union_member(&cur, tested) {
                env.insert(nw.var.clone(), narrowed);
            }
        }
        NarrowKind::Truthy => {}
    }
}

/// Whether `a` and `b` name the same class for `is_a?` elimination
/// purposes, ignoring any generic element type (bead ita-zdy): the core
/// narrowing target for `is_a?(Array)` is always `Ty::Array(Unknown)`
/// (see `core_const_ty`) regardless of what element type a union member
/// actually declares (e.g. `Array[String]`), so matching by full
/// `PartialEq` would silently never fire the elimination for `Array`/
/// `Hash` union members. Scalar/nominal variants have no parameter to
/// ignore, so they fall through to ordinary equality.
fn ty_class_eq(a: &Ty, b: &Ty) -> bool {
    match (a, b) {
        (Ty::Array(_), Ty::Array(_)) => true,
        (Ty::Hash(_, _), Ty::Hash(_, _)) => true,
        (Ty::Instance(x), Ty::Instance(y)) => x == y,
        (Ty::Class(x), Ty::Class(y)) => x == y,
        _ => a == b,
    }
}

/// Eliminate `tested` from a CLOSED union after `x.is_a?(tested)` proved
/// false (bead ita-zdy). "Closed" here means every member of `cur` is a
/// concrete type — none is `Ty::Unknown`. In practice `Ty::union` (the
/// only place a `Ty::Union` is ever built, see `types.rs`) already makes
/// that automatic: unioning anything with `Unknown` collapses straight
/// to `Unknown` rather than ever producing a `Union` that contains it.
/// The check below is deliberately still explicit rather than relying on
/// that construction detail: this function is the one place allowed to
/// narrow a false `is_a?` branch, and it must never manufacture a
/// concrete type out of a receiver that could in fact be `Unknown` at
/// runtime (invariant #1) — belt-and-suspenders against that invariant
/// ever changing upstream. Returns `None` (meaning: leave the type
/// exactly as `base` had it, the original "no else info" behavior) when
/// `cur` isn't a union, a member is `Unknown`, or `tested` doesn't match
/// any member; returns the narrowed type otherwise — the remaining
/// single member unwrapped out of `Ty::Union`, or the smaller union when
/// more than one member survives.
fn eliminate_union_member(cur: &Ty, tested: &Ty) -> Option<Ty> {
    let Ty::Union(parts) = cur else {
        return None;
    };
    if parts.contains(&Ty::Unknown) {
        return None;
    }
    if !parts.iter().any(|p| ty_class_eq(p, tested)) {
        return None;
    }
    let remaining: Vec<Ty> = parts.iter().filter(|p| !ty_class_eq(p, tested)).cloned().collect();
    match remaining.as_slice() {
        [] => None,
        [single] => Some(single.clone()),
        _ => Some(Ty::Union(remaining)),
    }
}

/// Remove `Nil` from a type after a `nil?`-false fact. Only ever narrows
/// an existing `Nil`/`Union` — never invents a concrete type out of
/// `Unknown`, so this can never manufacture a false positive (invariant
/// #1): a var that was `Unknown` before the check stays `Unknown` after.
fn strip_nil(t: Ty) -> Ty {
    match t {
        Ty::Nil => Ty::Unknown,
        Ty::Union(parts) => {
            let remaining: Vec<Ty> = parts.into_iter().filter(|p| *p != Ty::Nil).collect();
            match remaining.as_slice() {
                [] => Ty::Unknown,
                [single] => single.clone(),
                _ => Ty::Union(remaining),
            }
        }
        other => other,
    }
}

/// Does an `if`-branch's last statement unconditionally leave the
/// enclosing method (`return`/`break`/`next`/`redo`/`retry`, or a bare
/// `raise`/`fail` call)? Used only to decide whether the state after a
/// narrowing `if <fact> ... end` with no `else` should carry the
/// *negated* fact forward — the `return if x.nil?` / `raise ... if
/// x.nil?` early-return idiom (bead ita-u1t) — instead of joining. Checks
/// only the last statement: exactly the shape of those two idioms; a
/// return/raise buried earlier in a multi-statement branch does not make
/// the branch itself diverge.
fn stmts_diverge(stmts: &ruby_prism::StatementsNode<'_>) -> bool {
    let Some(last) = stmts.body().iter().last() else {
        return false;
    };
    match &last {
        Node::ReturnNode { .. }
        | Node::BreakNode { .. }
        | Node::NextNode { .. }
        | Node::RedoNode { .. }
        | Node::RetryNode { .. } => true,
        Node::CallNode { .. } => {
            let Some(call) = last.as_call_node() else {
                return false;
            };
            call.receiver().is_none() && matches!(call.name().as_slice(), b"raise" | b"fail")
        }
        _ => false,
    }
}

/// Return conformance is narrower than inference: closures, ensure overrides,
/// loops and unreachable suffixes must not manufacture an E0109.
#[derive(Default)]
struct ReturnContractSafety {
    uncertain: bool,
    /// Some `return` is written as `return` or `return nil`: the only
    /// `nil` an explicit return may be accused of (see `contract_accuses`).
    returns_literal_nil: bool,
}

impl<'pr> Visit<'pr> for ReturnContractSafety {
    fn visit_return_node(&mut self, node: &ruby_prism::ReturnNode<'pr>) {
        let literal = match node.arguments() {
            None => true,
            Some(args) => {
                let values: Vec<Node<'pr>> = args.arguments().iter().collect();
                matches!(values.as_slice(), [Node::NilNode { .. }])
            }
        };
        self.returns_literal_nil |= literal;
        ruby_prism::visit_return_node(self, node);
    }
    fn visit_block_node(&mut self, _: &ruby_prism::BlockNode<'pr>) {
        self.uncertain = true;
    }
    fn visit_lambda_node(&mut self, _: &ruby_prism::LambdaNode<'pr>) {
        self.uncertain = true;
    }
    fn visit_ensure_node(&mut self, _: &ruby_prism::EnsureNode<'pr>) {
        self.uncertain = true;
    }
    fn visit_rescue_node(&mut self, _: &ruby_prism::RescueNode<'pr>) {
        self.uncertain = true;
    }
    fn visit_while_node(&mut self, _: &ruby_prism::WhileNode<'pr>) {
        self.uncertain = true;
    }
    fn visit_until_node(&mut self, _: &ruby_prism::UntilNode<'pr>) {
        self.uncertain = true;
    }
    fn visit_for_node(&mut self, _: &ruby_prism::ForNode<'pr>) {
        self.uncertain = true;
    }
    fn visit_def_node(&mut self, _: &DefNode<'pr>) {
        self.uncertain = true;
    }
    fn visit_if_node(&mut self, node: &ruby_prism::IfNode<'pr>) {
        if matches!(node.predicate(), Node::TrueNode { .. } | Node::FalseNode { .. } | Node::NilNode { .. }) {
            self.uncertain = true;
        }
        ruby_prism::visit_if_node(self, node);
    }
    fn visit_unless_node(&mut self, node: &ruby_prism::UnlessNode<'pr>) {
        if matches!(node.predicate(), Node::TrueNode { .. } | Node::FalseNode { .. } | Node::NilNode { .. }) {
            self.uncertain = true;
        }
        ruby_prism::visit_unless_node(self, node);
    }
    fn visit_statements_node(&mut self, node: &ruby_prism::StatementsNode<'pr>) {
        let mut terminal = false;
        for statement in &node.body() {
            if terminal {
                self.uncertain = true;
            }
            self.visit(&statement);
            terminal = return_terminal(&statement);
        }
    }
}

/// Downcasts decide the shape here rather than the node discriminant: a
/// failed downcast is a node this walk cannot read, which is not a proven
/// terminal return — the same fail-closed answer as an unrecognized node.
fn return_terminal(node: &Node<'_>) -> bool {
    if matches!(node, Node::ReturnNode { .. }) {
        return true;
    }
    if let Some(statements) = node.as_statements_node() {
        return statements.body().iter().last().is_some_and(|n| return_terminal(&n));
    }
    if let Some(else_node) = node.as_else_node() {
        return else_node.statements().is_some_and(|n| return_terminal(&n.as_node()));
    }
    if let Some(if_node) = node.as_if_node() {
        return if_node.statements().is_some_and(|n| return_terminal(&n.as_node()))
            && if_node.subsequent().is_some_and(|n| return_terminal(&n));
    }
    if let Some(call) = node.as_call_node() {
        return call.receiver().is_none() && matches!(call.name().as_slice(), b"raise" | b"fail");
    }
    false
}

/// Pessimistic widening: pattern matching etc. may rebind anything.
fn widen_env(env: &mut Env) {
    for v in env.values_mut() {
        *v = Ty::Unknown;
    }
}

/// After a loop body: any var the body wrote becomes the union of before and
/// after (the body may run 0..n times).
fn merge_loop_env(base: &mut Env, body: &Env) {
    for (k, v) in body {
        let merged = match base.get(k) {
            Some(prev) if prev == v => continue,
            Some(prev) => Ty::union(prev.clone(), v.clone()),
            None => Ty::union(Ty::Nil, v.clone()),
        };
        base.insert(k.clone(), merged);
    }
}

/// Blocks close over locals: writes inside the block widen the outer var.
fn spill_block_writes(outer: &mut Env, block_env: &Env) {
    for (k, v) in block_env {
        match outer.get(k) {
            Some(prev) if prev == v => {}
            Some(prev) => {
                let u = Ty::union(prev.clone(), v.clone());
                outer.insert(k.clone(), u);
            }
            None => {} // block-local: stays local
        }
    }
}

/// Bind block/lambda parameters into `env` (all `Ty::Unknown`, blocks
/// never carry a declared type). `sink` (bead ita-au5) — when `Some` —
/// also collects every bound name (params, rest, keywords, and `; x`
/// block-locals alike) so `Checker::unknown_origin` can recognize a
/// later `LocalVariableReadNode` read of one as a block param; `None`
/// costs nothing beyond the `Option` check.
fn add_block_params(env: &mut Env, params: Option<&Node<'_>>, mut sink: Option<&mut Vec<String>>) {
    let Some(p) = params else { return };
    if let Some(bp) = p.as_block_parameters_node() {
        if let Some(inner) = bp.parameters() {
            for req in &inner.requireds() {
                if let Some(r) = req.as_required_parameter_node() {
                    let name = String::from_utf8_lossy(r.name().as_slice()).into_owned();
                    if let Some(s) = sink.as_deref_mut() {
                        s.push(name.clone());
                    }
                    env.insert(name, Ty::Unknown);
                }
            }
            for opt in &inner.optionals() {
                if let Some(o) = opt.as_optional_parameter_node() {
                    let name = String::from_utf8_lossy(o.name().as_slice()).into_owned();
                    if let Some(s) = sink.as_deref_mut() {
                        s.push(name.clone());
                    }
                    env.insert(name, Ty::Unknown);
                }
            }
            if let Some(rest) = inner.rest() {
                if let Some(r) = rest.as_rest_parameter_node() {
                    if let Some(n) = r.name() {
                        let name = String::from_utf8_lossy(n.as_slice()).into_owned();
                        if let Some(s) = sink.as_deref_mut() {
                            s.push(name.clone());
                        }
                        env.insert(name, Ty::Unknown);
                    }
                }
            }
            for kw in &inner.keywords() {
                let name = if let Some(k) = kw.as_required_keyword_parameter_node() {
                    Some(String::from_utf8_lossy(k.name().as_slice()).into_owned())
                } else { kw.as_optional_keyword_parameter_node().map(|k| String::from_utf8_lossy(k.name().as_slice()).into_owned()) };
                if let Some(n) = name {
                    if let Some(s) = sink.as_deref_mut() {
                        s.push(n.clone());
                    }
                    env.insert(n, Ty::Unknown);
                }
            }
        }
        for local in &bp.locals() {
            if let Some(l) = local.as_block_local_variable_node() {
                let name = String::from_utf8_lossy(l.name().as_slice()).into_owned();
                if let Some(s) = sink.as_deref_mut() {
                    s.push(name.clone());
                }
                env.insert(name, Ty::Unknown);
            }
        }
    }
}

/// Numbered implicit block params (`_1`..`_9`, bead ita-au5): the only
/// block binding `add_block_params` never sees an explicit declaration
/// for, so `Checker::unknown_origin` falls back to name matching.
fn is_numbered_block_param(name: &str) -> bool {
    matches!(
        name,
        "_1" | "_2" | "_3" | "_4" | "_5" | "_6" | "_7" | "_8" | "_9"
    )
}

/// The literal shapes E0108 accepts as self-proving operands.
/// Deliberately short: an `Integer`/`Float`/`String`/`nil` literal, plus
/// interpolation — `"R$ #{price}"` is a `String` at runtime whatever it
/// interpolates, which is exactly what makes the accusing fixture a real
/// MRI `TypeError`. Everything else (a call, a constant, an array, a
/// backtick command, a `Rational`) is not a literal this check reasons
/// about, and an unrecognized right-hand side POISONS the local it is
/// written to rather than leaving it proven.
fn literal_operand_ty(node: &Node<'_>) -> Option<Ty> {
    match node {
        Node::IntegerNode { .. } => Some(Ty::Int),
        Node::FloatNode { .. } => Some(Ty::Float),
        Node::StringNode { .. } | Node::InterpolatedStringNode { .. } => Some(Ty::Str),
        Node::NilNode { .. } => Some(Ty::Nil),
        _ => None,
    }
}

/// E0108's flow-insensitive proof for ONE Ruby local scope: `name -> Ty`
/// for every local whose every write inside that scope is a literal of
/// the SAME core type and which nothing else can rebind.
///
/// Conservative by construction. A name written twice with different
/// literal types, written from a non-literal, op-assigned (`+=`, `||=`),
/// multi-assigned, bound as a `for`/pattern/`rescue =>` target, declared
/// block-local, or shadowed by a parameter is POISONED and never
/// returned; and one `eval`-family call anywhere in the scope drops the
/// whole map, because a string `eval` rewrites locals no AST scan can
/// read.
///
/// Ruby's own scope gates are respected rather than approximated: `def`,
/// `class`, `module` and `class << self` each open a fresh local scope,
/// so the scan never descends into them — writes in there cannot reach
/// this scope's locals, and the walker builds a separate map when it
/// enters one. Blocks and lambdas DO share the enclosing scope, so the
/// scan walks into them and their parameters poison the names they
/// shadow.
fn prove_operand_locals(
    params: Option<&Node<'_>>,
    body: Option<&Node<'_>>,
) -> FxHashMap<String, Ty> {
    let mut scan = OperandLocalScan::default();
    if let Some(params) = params {
        scan.visit(params);
    }
    if let Some(body) = body {
        scan.visit(body);
    }
    if scan.bail {
        return FxHashMap::default();
    }
    scan.seen
        .into_iter()
        .filter_map(|(name, ty)| ty.map(|t| (name, t)))
        .collect()
}

/// The scan behind `prove_operand_locals`. `None` in `seen` is poison,
/// and poison never lifts.
#[derive(Default)]
struct OperandLocalScan {
    seen: FxHashMap<String, Option<Ty>>,
    /// An `eval`-family call was seen: nothing in this scope is provable.
    bail: bool,
}

impl OperandLocalScan {
    /// Record a write of `ty` (`None` = a right-hand side this check
    /// cannot read). The first write takes the slot; any later write
    /// that disagrees — different type, or unreadable — poisons it, and
    /// a poisoned slot never recovers because poison disagrees with
    /// every `Some`.
    ///
    /// Written with `get_mut` before `insert` on purpose: a real file
    /// binds the same handful of names over and over, and the lookup key
    /// only has to be OWNED the first time each name is seen. Allocating
    /// per write instead cost a measurable slice of the check phase,
    /// which this scan runs once per scope.
    fn write(&mut self, name: &str, ty: Option<&Ty>) {
        if let Some(slot) = self.seen.get_mut(name) {
            if slot.as_ref() != ty {
                *slot = None;
            }
            return;
        }
        self.seen.insert(name.to_string(), ty.cloned());
    }

    fn poison(&mut self, name: &str) {
        if let Some(slot) = self.seen.get_mut(name) {
            *slot = None;
            return;
        }
        self.seen.insert(name.to_string(), None);
    }

    fn poison_id(&mut self, name: &ruby_prism::ConstantId<'_>) {
        self.poison(&String::from_utf8_lossy(name.as_slice()));
    }
}

impl<'pr> Visit<'pr> for OperandLocalScan {
    fn visit_local_variable_write_node(&mut self, node: &ruby_prism::LocalVariableWriteNode<'pr>) {
        let name = String::from_utf8_lossy(node.name().as_slice());
        self.write(&name, literal_operand_ty(&node.value()).as_ref());
        ruby_prism::visit_local_variable_write_node(self, node);
    }

    fn visit_local_variable_operator_write_node(
        &mut self,
        node: &ruby_prism::LocalVariableOperatorWriteNode<'pr>,
    ) {
        self.poison_id(&node.name());
        ruby_prism::visit_local_variable_operator_write_node(self, node);
    }

    fn visit_local_variable_and_write_node(
        &mut self,
        node: &ruby_prism::LocalVariableAndWriteNode<'pr>,
    ) {
        self.poison_id(&node.name());
        ruby_prism::visit_local_variable_and_write_node(self, node);
    }

    fn visit_local_variable_or_write_node(
        &mut self,
        node: &ruby_prism::LocalVariableOrWriteNode<'pr>,
    ) {
        self.poison_id(&node.name());
        ruby_prism::visit_local_variable_or_write_node(self, node);
    }

    /// Every non-`=` binding form lands here: a multi-assignment target,
    /// a `for` index, a `rescue => e` reference, a pattern-match capture.
    fn visit_local_variable_target_node(
        &mut self,
        node: &ruby_prism::LocalVariableTargetNode<'pr>,
    ) {
        self.poison_id(&node.name());
        ruby_prism::visit_local_variable_target_node(self, node);
    }

    fn visit_block_local_variable_node(&mut self, node: &ruby_prism::BlockLocalVariableNode<'pr>) {
        self.poison_id(&node.name());
        ruby_prism::visit_block_local_variable_node(self, node);
    }

    fn visit_required_parameter_node(&mut self, node: &ruby_prism::RequiredParameterNode<'pr>) {
        self.poison_id(&node.name());
        ruby_prism::visit_required_parameter_node(self, node);
    }

    fn visit_optional_parameter_node(&mut self, node: &ruby_prism::OptionalParameterNode<'pr>) {
        self.poison_id(&node.name());
        ruby_prism::visit_optional_parameter_node(self, node);
    }

    fn visit_rest_parameter_node(&mut self, node: &ruby_prism::RestParameterNode<'pr>) {
        if let Some(name) = node.name() {
            self.poison_id(&name);
        }
        ruby_prism::visit_rest_parameter_node(self, node);
    }

    fn visit_keyword_rest_parameter_node(
        &mut self,
        node: &ruby_prism::KeywordRestParameterNode<'pr>,
    ) {
        if let Some(name) = node.name() {
            self.poison_id(&name);
        }
        ruby_prism::visit_keyword_rest_parameter_node(self, node);
    }

    fn visit_required_keyword_parameter_node(
        &mut self,
        node: &ruby_prism::RequiredKeywordParameterNode<'pr>,
    ) {
        self.poison_id(&node.name());
        ruby_prism::visit_required_keyword_parameter_node(self, node);
    }

    fn visit_optional_keyword_parameter_node(
        &mut self,
        node: &ruby_prism::OptionalKeywordParameterNode<'pr>,
    ) {
        self.poison_id(&node.name());
        ruby_prism::visit_optional_keyword_parameter_node(self, node);
    }

    fn visit_block_parameter_node(&mut self, node: &ruby_prism::BlockParameterNode<'pr>) {
        if let Some(name) = node.name() {
            self.poison_id(&name);
        }
        ruby_prism::visit_block_parameter_node(self, node);
    }

    /// `_1`..`_9` are bound by the block with no parameter node to see,
    /// so every one of them is poisoned wherever numbered params appear.
    fn visit_numbered_parameters_node(&mut self, node: &ruby_prism::NumberedParametersNode<'pr>) {
        for i in 1..=9 {
            self.poison(&format!("_{i}"));
        }
        ruby_prism::visit_numbered_parameters_node(self, node);
    }

    /// A string `eval` (or a `binding`/`local_variable_set` reached
    /// through one) can rewrite any local in this scope with content no
    /// AST scan can read: the whole scope stops being provable.
    fn visit_call_node(&mut self, node: &ruby_prism::CallNode<'pr>) {
        if matches!(
            node.name().as_slice(),
            b"eval"
                | b"instance_eval"
                | b"class_eval"
                | b"module_eval"
                | b"binding"
                | b"local_variable_set"
        ) {
            self.bail = true;
        }
        ruby_prism::visit_call_node(self, node);
    }

    // The four scope gates below: a local written inside one of these is
    // never the local read outside it (Ruby opens a fresh local scope
    // there), so descending would poison names provably untouched.
    fn visit_def_node(&mut self, _node: &ruby_prism::DefNode<'pr>) {}

    fn visit_class_node(&mut self, _node: &ruby_prism::ClassNode<'pr>) {}

    fn visit_module_node(&mut self, _node: &ruby_prism::ModuleNode<'pr>) {}

    fn visit_singleton_class_node(&mut self, _node: &ruby_prism::SingletonClassNode<'pr>) {}
}

fn span_of_call(call: &CallNode<'_>) -> (usize, usize) {
    let loc = call.location();
    (loc.start_offset(), loc.end_offset())
}

/// Find the `DefNode` whose full span equals `span`, searching the same
/// container shapes the index walks.
fn find_def_at<'pr>(node: &Node<'pr>, span: (usize, usize)) -> Option<DefNode<'pr>> {
    if let Some(def) = node.as_def_node() {
        let loc = def.location();
        if (loc.start_offset(), loc.end_offset()) == span {
            return Some(def);
        }
        return None;
    }
    let children: Vec<Node<'pr>> = if let Some(p) = node.as_program_node() {
        vec![p.statements().as_node()]
    } else if let Some(s) = node.as_statements_node() {
        s.body().iter().collect()
    } else if let Some(b) = node.as_begin_node() {
        b.statements().map(|s| s.as_node()).into_iter().collect()
    } else if let Some(c) = node.as_class_node() {
        c.body().into_iter().collect()
    } else if let Some(m) = node.as_module_node() {
        m.body().into_iter().collect()
    } else if let Some(sc) = node.as_singleton_class_node() {
        sc.body().into_iter().collect()
    } else if let Some(i) = node.as_if_node() {
        i.statements()
            .map(|s| s.as_node())
            .into_iter()
            .chain(i.subsequent())
            .collect()
    } else if let Some(u) = node.as_unless_node() {
        u.statements()
            .map(|s| s.as_node())
            .into_iter()
            .chain(u.else_clause().map(|e| e.as_node()))
            .collect()
    } else {
        let e = node.as_else_node()?;
        e.statements().map(|s| s.as_node()).into_iter().collect()
    };
    for child in children {
        let loc = child.location();
        if loc.start_offset() <= span.0 && span.1 <= loc.end_offset() {
            if let Some(found) = find_def_at(&child, span) {
                return Some(found);
            }
        }
    }
    None
}

fn join_path(scope: &str, name: &str) -> String {
    if scope.is_empty() || name.starts_with("::") {
        name.trim_start_matches("::").to_string()
    } else {
        format!("{scope}::{name}")
    }
}

/// Plain Levenshtein over chars (w12 closure). Did-you-mean only — never
/// on any path that decides whether a diagnostic fires. Hand-rolled
/// because it is ~15 lines; a crate would be the workspace's only new
/// dependency for it. Saturates above 2: callers only ever compare
/// against a cutoff of 2, so distant pairs abandon the DP early and
/// report 3 — never a true distance above 2.
fn edit_distance(a: &str, b: &str) -> usize {
    // Length gap alone proves distance > 2 — skip the DP entirely. Every
    // caller only compares against a cutoff of 2, so a saturated 3 is as
    // good as the true distance.
    let la = a.chars().count();
    let lb = b.chars().count();
    if la.abs_diff(lb) > 2 {
        return 3;
    }
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0usize; b.len() + 1];
    for (i, ca) in a.chars().enumerate() {
        cur[0] = i + 1;
        let mut row_min = cur[0];
        for (j, cb) in b.iter().enumerate() {
            cur[j + 1] = (prev[j + 1] + 1)
                .min(cur[j] + 1)
                .min(prev[j] + usize::from(ca != *cb));
            row_min = row_min.min(cur[j + 1]);
        }
        // Early abandon: distance ≤ 2 requires every row's minimum ≤ 2
        // (final distance can only grow), so a row minimum above the
        // cutoff settles it. This is what keeps E0104's did-you-mean
        // affordable at corpus scale — thousands of warnings times
        // thousands of candidates never run full DP.
        if row_min > 2 {
            return 3;
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// Best did-you-mean candidate for `typo` among `candidates`: smallest
/// edit distance, ties broken lexicographically (so `HashMap` iteration
/// order can never leak into output), capped at distance 2. The identical
/// spelling is never suggested — a 0-distance candidate means the name
/// exists but did not resolve in this scope, and echoing it back is not a
/// suggestion.
/// Const-suggestion selection (w12): the winner is the closest candidate by
/// (edit distance, KEY) — the tuple compares the key, not just the distance,
/// because several qualified paths share one bare display (`Foo::Bar` and
/// `Baz::Bar` both display `Bar`) and `by_path`/`consts` iterate in `HashMap`
/// order. Without the key in the tie-break the first-seen winner (and its
/// defined-at line) flipped run to run — measured nondeterminism during the
/// ita-6dh determinism proof, present on the pristine pre-threading binary.
fn closest_const_key<'a>(
    typo: &str,
    candidates: impl Iterator<Item = (&'a str, &'a str)>,
) -> Option<&'a str> {
    let mut best: Option<(usize, &str)> = None;
    for (shown, key) in candidates {
        if shown == typo {
            continue;
        }
        let d = edit_distance(typo, shown);
        if d > 2 {
            continue;
        }
        if best.is_none_or(|(bd, bk)| (d, key) < (bd, bk)) {
            best = Some((d, key));
        }
    }
    best.map(|(_, key)| key)
}

fn closest_name<'a>(typo: &str, candidates: impl Iterator<Item = &'a str>) -> Option<&'a str> {
    let mut best: Option<(usize, &str)> = None;
    for cand in candidates {
        if cand == typo {
            continue;
        }
        let d = edit_distance(typo, cand);
        if d > 2 {
            continue;
        }
        if best.is_none_or(|(bd, bn)| (d, cand) < (bd, bn)) {
            best = Some((d, cand));
        }
    }
    best.map(|(_, name)| name)
}

#[cfg(test)]
mod suggestion_tests {
    use super::{closest_const_key, closest_name, edit_distance};

    #[test]
    fn distance_two_is_the_cutoff() {
        assert_eq!(edit_distance("calculat", "calculate"), 1);
        assert_eq!(edit_distance("nmae", "name"), 2);
        assert_eq!(edit_distance("abc", "xyz"), 3);
        assert_eq!(
            closest_name("nmae", ["name", "nanme"].into_iter()),
            Some("name")
        );
        assert_eq!(closest_name("abc", ["xyz"].into_iter()), None);
    }

    #[test]
    fn const_ties_break_by_key_regardless_of_iteration_order() {
        // `Bar` is distance 0-display tie for two qualified keys (the bare
        // path `Barr` is distance 1 from both displays): iteration order
        // must never change the winner — the lexicographically smaller KEY
        // wins, both directions. Mutant `(d, key) < (bd, bk)` -> `d < bd`:
        // the reversed case fails (first-seen `Zzz::Bar` would win).
        let fwd = [("Bar", "Zzz::Bar"), ("Bar", "Aaa::Bar")];
        let rev = [("Bar", "Aaa::Bar"), ("Bar", "Zzz::Bar")];
        assert_eq!(closest_const_key("Barr", fwd.into_iter()), Some("Aaa::Bar"));
        assert_eq!(closest_const_key("Barr", rev.into_iter()), Some("Aaa::Bar"));
    }

    #[test]
    fn ties_break_lexicographically_and_identity_is_never_a_suggestion() {
        // `calcu` is distance 1 from both candidates: the lower name wins,
        // so HashMap iteration order can never leak into a diagnostic.
        assert_eq!(
            closest_name("calcu", ["zalcu", "calc"].into_iter()),
            Some("calc")
        );
        // A 0-distance "candidate" means the name exists but didn't
        // resolve here — echoing it back is not a suggestion.
        assert_eq!(closest_name("calc", ["calc"].into_iter()), None);
    }
}

/// Direct unit tests of `eliminate_union_member` (bead ita-zdy) — the
/// closed-union `is_a?`-false elimination — independent of the full
/// checker pipeline. `crates/itaruby_semantic/tests/union_elimination.rs`
/// covers the same feature end-to-end through real `.rb` fixtures; this
/// module exists specifically because one control (a union containing
/// `Ty::Unknown`) can never actually arise from real source — every
/// `Ty::Union` in this crate is built through `Ty::union`, which
/// collapses to `Ty::Unknown` outright rather than ever producing a
/// `Union` that contains it (see `types.rs`) — so the only way to prove
/// the defensive guard in `eliminate_union_member` actually holds is to
/// hand-construct that impossible-in-practice value directly.
#[cfg(test)]
mod union_elimination_tests {
    use super::{eliminate_union_member, ty_class_eq};
    use crate::types::{ClassId, Ty};

    /// FIRES the elimination: `String | Array[Unknown]` minus `Array`
    /// (the core `is_a?(Array)` narrowing target, see `core_const_ty`)
    /// leaves exactly `String` — the single-member case unwraps out of
    /// `Ty::Union` entirely, matching `strip_nil`'s sibling contract.
    #[test]
    fn two_member_union_narrows_to_the_remaining_single_member() {
        let cur = Ty::Union(vec![Ty::Str, Ty::Array(Box::new(Ty::Unknown))]);
        let tested = Ty::Array(Box::new(Ty::Unknown));
        assert_eq!(
            eliminate_union_member(&cur, &tested),
            Some(Ty::Str),
            "eliminating one of two members must unwrap to the bare remaining type"
        );
    }

    /// FIRES: a three-member union keeps the other two as a smaller
    /// `Ty::Union`, not a single type and not the original union.
    #[test]
    fn three_member_union_narrows_to_a_smaller_union() {
        let a = Ty::Instance(ClassId(1));
        let b = Ty::Instance(ClassId(2));
        let c = Ty::Instance(ClassId(3));
        let cur = Ty::Union(vec![a.clone(), b.clone(), c.clone()]);
        assert_eq!(
            eliminate_union_member(&cur, &b),
            Some(Ty::Union(vec![a, c])),
            "eliminating one of three members must keep exactly the other two"
        );
    }

    /// Element-type-agnostic matching (bead ita-zdy): `is_a?(Array)`
    /// always narrows to `Ty::Array(Unknown)` regardless of what element
    /// type a union member actually carries — `ty_class_eq` (and
    /// therefore the elimination) must match `Array[String]` against it
    /// by class shape, not by full equality.
    #[test]
    fn array_and_hash_members_match_by_shape_not_element_type() {
        assert!(ty_class_eq(
            &Ty::Array(Box::new(Ty::Str)),
            &Ty::Array(Box::new(Ty::Unknown))
        ));
        assert!(ty_class_eq(
            &Ty::Hash(Box::new(Ty::Sym), Box::new(Ty::Int)),
            &Ty::Hash(Box::new(Ty::Unknown), Box::new(Ty::Unknown))
        ));
        assert!(!ty_class_eq(&Ty::Instance(ClassId(1)), &Ty::Instance(ClassId(2))));
    }

    /// SILENT (returns `None`, meaning "leave the type exactly as it
    /// was"): `tested` isn't a member of the union at all — nothing to
    /// eliminate.
    #[test]
    fn tested_type_absent_from_union_yields_no_narrowing() {
        let cur = Ty::Union(vec![Ty::Str, Ty::Int]);
        assert_eq!(eliminate_union_member(&cur, &Ty::Sym), None);
    }

    /// SILENT: a non-union receiver (e.g. plain `Ty::Unknown`, or an
    /// already-concrete `Ty::Instance`) never gets touched — matches the
    /// existing "no else info" contract for anything that isn't a union.
    #[test]
    fn non_union_receiver_yields_no_narrowing() {
        assert_eq!(eliminate_union_member(&Ty::Unknown, &Ty::Str), None);
        assert_eq!(
            eliminate_union_member(&Ty::Instance(ClassId(1)), &Ty::Instance(ClassId(1))),
            None
        );
    }

    /// THE guard this bead's invariant #1 hinges on, proven directly
    /// (test 3's control, `union_elimination.rs`'s own doc comment
    /// explains why this can't be exercised through a real fixture): a
    /// union containing `Ty::Unknown` — hand-built here, since
    /// `Ty::union` itself can never produce one — must NEVER be narrowed,
    /// even though `tested` genuinely is one of its other members.
    /// MUTANT this test catches: dropping the `parts.iter().any(|p| *p
    /// == Ty::Unknown)` guard from `eliminate_union_member` would narrow
    /// this down to `Ty::Str`, manufacturing a concrete type out of a
    /// receiver that could be anything at runtime.
    #[test]
    fn union_containing_unknown_is_never_narrowed() {
        let cur = Ty::Union(vec![Ty::Str, Ty::Unknown]);
        assert_eq!(
            eliminate_union_member(&cur, &Ty::Str),
            None,
            "a union containing Unknown must never be eliminated from, even when `tested` matches a concrete member"
        );
    }
}
