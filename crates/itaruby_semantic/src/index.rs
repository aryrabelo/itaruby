//! Definition index: per-file extraction (`file_defs`) merged into a global
//! `project_index`. Incrementality invariant: `file_defs` depends only on its
//! own file's text; editing a method body without changing signatures yields
//! a structurally-equal `FileDefs`, so salsa early-cutoff keeps the global
//! index untouched.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rustc_hash::{FxHashMap, FxHashSet};

use itaruby_syntax::ruby_prism::{self, Node, Visit};
use itaruby_syntax::{LineIndex, SourceFile};

use crate::core;
use crate::rbs_comment::{parse_rbs_comment, RbsSig};
use crate::types::{ClassId, Ty};
use crate::ProjectFiles;

/// `self.table_name = ...` as written in a model, distinguishing a literal
/// override (bead ita-yho's model->table contract gives it priority over
/// convention) from a dynamic one (silence: a dynamic override means the
/// convention guess cannot be trusted either).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TableNameDecl {
    Literal(String),
    Dynamic,
}

/// Why a class fragment was marked `open` (bead ita-anc, census only — see
/// `ProjectIndex::inconclusive_reason`). Recorded at every `open = true`
/// site in `DefWalker`/`merge_declared_fragment`; first reason wins when
/// fragments merge (a class reopened in two files, or opened for two
/// reasons in one file, keeps whichever came first).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OpenReason {
    /// Superclass expression is not a resolvable constant path (`< Struct.new(...)`).
    DynamicSuperclass,
    /// Defines `method_missing` / `respond_to_missing?` — either in the
    /// fragment's OWN body (`DefWalker`'s `DefNode` arm) or through a
    /// mixin edge this index could attribute to it
    /// (`apply_attributed_mixin_edges`, the receiver-keyed half of the
    /// dynamic-mixin family). Both mean the same thing: this class
    /// answers every name, so nothing about its surface is knowable.
    MethodMissing,
    /// The `raise NotImplementedError` abstract-class text-scan idiom.
    AbstractRaise,
    /// `include`/`extend`/`prepend` with a non-self explicit receiver.
    DynamicMixinReceiver,
    /// `include`/`extend`/`prepend` with a non-constant-path argument.
    DynamicMixinArg,
    /// A symbol-list class-body call (`attr_reader`/`attr_writer`/
    /// `attr_accessor`) with a non-symbol arg. Real visibility modifiers
    /// (`private`/`public`/`protected`) never open a class at all — they
    /// only recurse into a wrapped `def` — so despite the earlier name
    /// this variant is exclusively the `attr_*` shape.
    DynamicAttrArg,
    /// `define_method` with a non-literal name.
    DynamicDefineMethod,
    /// `alias_method` with a non-literal name.
    DynamicAliasMethod,
    /// `class_eval`/`module_eval`/`instance_eval`/`send`/`public_send`/
    /// `__send__`/`delegate`/`define_singleton_method` — plus, from the
    /// onda-2 beads, two merge-time passes that reach the same shape
    /// without an AST: `apply_load_hook_openness`
    /// (`run_load_hooks(:sym, Base)` → `base.class_eval(&block)`) and
    /// `apply_extended_hooks` (a `self.extended(base)` body whose
    /// installs onto `base` cannot be enumerated).
    EvalOrSend,
    /// The catch-all `_ =>` arm: some class-body call we do not model.
    /// In a Rails app this is `validates` / `belongs_to` / `scope` /
    /// `before_action` — the population an allowlist or generated-method
    /// model could close.
    UnknownClassBodyCall,
    /// A block attached to a class-body call that is not `define_method`:
    /// `included do ... end` (`ActiveSupport::Concern`), `FIELDS.each do`.
    /// Split out of `UnknownClassBodyCall` on purpose (bead ita-o1n): it
    /// is a DIFFERENT fix — the block body has to be walked in class
    /// scope — and lumping the two hides which one the corpora need.
    ClassBodyBlock,
    /// `class << <non-self expr>`: a singleton class on something we
    /// cannot resolve. Not a class-body call at all; own variant so the
    /// DSL bucket stays honest.
    SingletonClassExpr,
    /// `merge_declared_fragment`: `declarations/gems.rbi` force-opens the entry.
    DeclaredExternal,
    /// Bead ita-h6l (mechanism B): `class X ... end` whose path `X` names a
    /// Ruby core class, a `declarations/stdlib_constants.txt` constant, or
    /// a `declarations/gems.rbi` namespace — a REOPENING of a class this
    /// checker never modeled, not the declaration of a brand-new closed
    /// project class. Set at index time in `DefWalker`'s `ClassNode` arm,
    /// by path only, independent of `closed_world`/`requires` (fail-closed:
    /// invariant #1 prefers silence on the rare name collision over the
    /// guaranteed false-positive cascade on the real reopening —
    /// `Pathname#exist?`, `Range#overlap?`, `Mail::Message#...`, none of
    /// which this checker's own inventory ever saw defined).
    ReopenedExternal,
    /// An ancestor that is `open` with no reason recorded — a bug in this
    /// attribution, not a Ruby construct. Exists so such a class shows up
    /// as a visible non-zero in `anc_other` instead of silently leaving
    /// the ancestry census entirely.
    Unattributed,
    /// Bead ita-nst: a `def self.x` KEYWORD nested inside an INSTANCE
    /// method's body (usually in a block): at runtime `self` there is
    /// one object, so the definition lands on that object's own singleton
    /// — an owner no index position can name. The enclosing class OPENS
    /// rather than collecting a name it may not have (invariant #1).
    NestedDefOwner,
    /// Bead ita-blk: a `class X`/`module X` KEYWORD written inside a
    /// class-body BLOCK (`test "..." do class Foo < Rails::Railtie ... end
    /// end`). The `class` keyword's cref is lexical, so at runtime this
    /// really defines `<enclosing nesting>::X` — but the walker never
    /// descended into the block, so the name was absent from the index
    /// and a later `X.some_method` resolved to an UNRELATED same-named
    /// class somewhere else in the project (measured on rails: four
    /// residue records where `Foo` inside `RailtiesTest::RailtieTest`
    /// resolved to `activesupport/test/testing/constant_lookup_test.rb:5`
    /// `class Foo; end`, an empty stub). Registering the fragment puts
    /// the name back where Ruby puts it; marking it open says the body
    /// this walk did not read is unknown. Monotonic both ways: a wrong
    /// receiver stops being consulted, and the right one answers
    /// `Inconclusive`.
    BlockNestedDefinition,
    /// Bead ita-src: the project writes Ruby source that defines a class
    /// of this name inside a STRING literal — rails' isolation tests do
    /// `app_file "app/models/foo.rb", <<-RUBY ... class Foo <
    /// ApplicationRecord ... RUBY` and then load the generated app. The
    /// parsed tree contains no such definition, so a later `Foo.x`
    /// resolved to whatever unrelated same-named stub the project
    /// happened to contain (`activesupport/test/testing/
    /// constant_lookup_test.rb:5 class Foo; end`) and read as a
    /// conclusive miss on code that runs. Gated on the resolved fragment
    /// being a BARE STUB (no methods and no singleton methods of its
    /// own): a real, fleshed-out class that merely appears in a
    /// generator template keeps its whole surface checkable.
    StringSourceDefined,
}

/// What actually blocks a `MethodLookup::Inconclusive` from concluding
/// (bead ita-anc, census only — never consulted by the real check path).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Blocker {
    /// An ancestor NAME in the chain never resolved in the index at all —
    /// we do not even know what it is. Strictly harder than `Declared`
    /// and it survives the `Declared` fix, so it wins whenever both are
    /// present: the counterfactual question is "would giving real methods
    /// to every declared external ancestor close this site?", and here the
    /// answer stays no.
    Unresolved,
    /// Every external blocker in the chain is a name we DID resolve, to a
    /// `declarations/gems.rbi` entry that `merge_declared_fragment`
    /// force-opens (bead ita-3gs declares the constant and deliberately
    /// nothing else). Closable by giving that name a real method set —
    /// Tapioca RBI or a curated list — without any new name resolution.
    Declared,
    /// Every open ancestor in the chain is project-side. THIS is the
    /// closable population.
    Project(OpenReason),
}

/// Public framework calls that appear in a class body, register a hook or
/// a validator, and define NO method on the class (bead ita-o1n). Every
/// name here is documented public API of Rails/ActiveJob/Sidekiq, so this
/// is the same category as `declarations/gems.rbi` — a declaration about a
/// public gem surface, never a lookup table of app symbols. The
/// anti-gaming rule still stands: a name that exists only in one client's
/// code must NEVER appear here.
///
/// The exclusions are the whole safety argument, because allowlisting a
/// call that DOES define something closes a class that should stay open
/// and turns every call to the generated method into a false `E0101`.
/// Deliberately absent, all method generators: `belongs_to`, `has_many`,
/// `has_one`, `has_and_belongs_to_many`, `scope`, `enum`, `attribute`,
/// `store`, `store_accessor`, `alias_attribute`, `composed_of`,
/// `accepts_nested_attributes_for`, `has_secure_password`,
/// `has_secure_token`, `has_one_attached`, `has_many_attached`,
/// `monetize`, `enumerize`, `devise`, every `acts_as_*`. `delegate` is
/// excluded too — it already opens the class via `EvalOrSend`.
///
/// ponytail: a flat name match, not a framework model. It cannot tell a
/// Rails `validates` from a same-named method on an unrelated class, and
/// it does not need to — a false match only ever CLOSES a class that a
/// real generator would have opened, which the corpus gate catches as a
/// new diagnostic. Upgrade path if that ever fires: require the enclosing
/// class to have a resolvable framework ancestor before trusting the name.
fn defines_no_method(name: &str) -> bool {
    matches!(
        name,
        // validations: register validators, define nothing
        "validates"
            | "validate"
            | "validates_with"
            | "validates_each"
            | "validates_presence_of"
            | "validates_uniqueness_of"
            | "validates_length_of"
            | "validates_format_of"
            | "validates_numericality_of"
            | "validates_inclusion_of"
            | "validates_exclusion_of"
            | "validates_associated"
            | "validates_acceptance_of"
            | "validates_confirmation_of"
            | "validates_absence_of"
            // ActiveRecord lifecycle callbacks
            | "before_validation"
            | "after_validation"
            | "before_save"
            | "after_save"
            | "around_save"
            | "before_create"
            | "after_create"
            | "around_create"
            | "before_update"
            | "after_update"
            | "around_update"
            | "before_destroy"
            | "after_destroy"
            | "around_destroy"
            | "after_commit"
            | "after_rollback"
            | "after_initialize"
            | "after_find"
            | "after_touch"
            // controller filters and config
            | "before_action"
            | "after_action"
            | "around_action"
            | "skip_before_action"
            | "skip_after_action"
            | "skip_around_action"
            | "prepend_before_action"
            | "prepend_after_action"
            | "prepend_around_action"
            | "rescue_from"
            | "protect_from_forgery"
            | "layout"
            | "http_basic_authenticate_with"
            // `helper_method` exposes an EXISTING method to views and
            // `helper` mixes a module into the view context — neither adds
            // anything to this class.
            | "helper"
            | "helper_method"
            // job / worker configuration
            | "queue_as"
            | "retry_on"
            | "discard_on"
            | "sidekiq_options"
            // ActiveRecord configuration that defines no accessor: the
            // column readers come from the schema (beads ita-yho/ita-muf),
            // these calls only annotate them.
            | "serialize"
            | "attr_readonly"
            | "default_scope"
    )
}

fn is_schema_rb_path(path: &std::path::Path) -> bool {
    path.file_name().and_then(|n| n.to_str()) == Some("schema.rb")
        && path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            == Some("db")
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MethodDef {
    pub name: String,
    pub required: u32,
    pub optional: u32,
    pub rest: bool,
    /// (name, required)
    pub keywords: Vec<(String, bool)>,
    pub kwrest: bool,
    pub block: bool,
    pub sig: Option<RbsSig>,
    /// Raw text of a preceding sorbet `sig { ... }`'s `.returns(...)`
    /// argument (bead ita-uh1) — e.g. `"T.nilable(String)"`. `None` for
    /// `.void`, for no sig at all, and for any sig shape `index.rs`'s
    /// `extract_sig_return` doesn't recognize. Deliberately separate from
    /// `sig` above (which comes from this project's own `#:` RBS
    /// comments and drives arity checking): a Tapioca-rendered RBI sig
    /// can go stale relative to the gem actually installed, so this
    /// field NEVER feeds arity — only `sorbet_sig::sorbet_ret_ty` ever
    /// consumes it, to type the call's return, never its parameters.
    pub sorbet_ret: Option<String>,
    /// If true, arity/args are unchecked (synthetic: `define_method`, alias).
    pub arity_unknown: bool,
    /// The def body contains `raise NotImplementedError` — an abstract
    /// stub (2026-09-03, rails dd1c8848^). The stub's own signature is
    /// not the one that runs: under the template-method idiom the runtime
    /// receiver is a family instance and any member redefining the name
    /// shadows the stub, so `check.rs` skips arity on a shadowed stub.
    pub abstract_stub: bool,
    pub name_span: (usize, usize),
    pub def_span: (usize, usize),
}

impl MethodDef {
    fn synthetic(name: String, required: u32, span: (usize, usize)) -> Self {
        MethodDef {
            name,
            required,
            optional: 0,
            rest: false,
            keywords: Vec::new(),
            kwrest: false,
            block: false,
            sig: None,
            sorbet_ret: None,
            arity_unknown: false,
            abstract_stub: false,
            name_span: span,
            def_span: span,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassFragment {
    /// Fully nested constant path, e.g. `Foo::Bar`.
    pub path: String,
    /// Real lexical `Module.nesting` chain in effect where this fragment's
    /// own body/header was written, outermost first, ENDING with this
    /// fragment's own `path` (bead ita-519). Distinct from splitting
    /// `path` on `::`: a compact-syntax `class A::B::C` pushes exactly ONE
    /// level (`nesting == [..outer, "A::B::C"]`), never separate `A`/`A::B`
    /// entries — the whole point this field exists for. See
    /// `ProjectIndex::resolve_const`'s doc comment for the bug this fixes.
    pub nesting: Vec<String>,
    pub is_module: bool,
    /// Superclass as written (constant path literal), if any.
    pub superclass: Option<String>,
    pub includes: Vec<String>,
    pub prepends: Vec<String>,
    pub extends: Vec<String>,
    /// `mixes_in_class_methods ::X` args (Sorbet `T::Helpers` — Tapioca's
    /// RBI rendering of the `included do extend X end` /
    /// `ActiveSupport::Concern` `ClassMethods` idiom). Never consulted by
    /// project-side method lookup (`X`'s methods really do land on the
    /// includer's singleton, exactly like a real `extend`, but modeling
    /// that for project code is out of scope here) — the call still
    /// force-opens the fragment exactly as before this field existed, so
    /// real-project ancestry stays as conservative as ever. Populated
    /// only so `compute_rbi_method_closure` (bead ita-xze) can walk it as
    /// a singleton-track edge, the same as a real `extend`.
    pub mixes_in_class_methods: Vec<String>,
    pub methods: Vec<MethodDef>,
    pub singleton_methods: Vec<MethodDef>,
    /// Simple names of constants assigned in this fragment.
    pub consts: Vec<String>,
    /// Metaprogramming detected: everything about this class is Unknown.
    pub open: bool,
    /// Why `open` is set (bead ita-anc); `None` while closed. First-reason-wins.
    pub open_reason: Option<OpenReason>,
    /// `self.table_name = ...` as written in this fragment, if any.
    pub table_name: Option<TableNameDecl>,
    /// This fragment was NOT written by a `class`/`module` keyword: it
    /// exists because some other file patched this path's singleton by
    /// NAME (`X.singleton_class.prepend M`, `class << X`). Merging it
    /// like an ordinary fragment would INVENT the class, and inventing
    /// one is a measured false-positive source: discourse's
    /// `lib/freedom_patches/final_destination_connect.rb` patches
    /// `TCPSocket.singleton_class`, and a fragment for `TCPSocket` made
    /// the stdlib class look like a closed project class with no methods
    /// — one new E0101 "undefined method `close`" at
    /// `spec/support/nginx_test_proxy.rb:145`, on code that runs.
    ///
    /// So these are held aside by `merge_file_fragments` and applied by
    /// `apply_singleton_patches` ONLY to a path the project really
    /// declares, after every file has merged (the same "resolve last"
    /// discipline as `resolve_qualified_const_writes`).
    pub declared_owner_required: bool,
    /// Bead H of onda 2: the names this fragment's `def self.extended(
    /// base)` hook installs on `base`'s INSTANCE surface, with the span
    /// of the statement that installs each — `base.delegate :model_name,
    /// to: :class` is the canonical one. `extend M` puts none of these on
    /// the extender by itself (it reads `M.methods` onto the extender's
    /// SINGLETON), so `apply_extended_hooks` copies them onto every class
    /// that really `extend`s this module. See `harvest_extended_hook` for
    /// the four shapes that count and the fail-closed rule.
    pub hook_instance_installs: Vec<(String, (usize, usize))>,
    /// The same hook's `def base.x` statements. Those install on the base
    /// OBJECT's singleton, never on its instances: registering them as
    /// instance methods would silence a real `NoMethodError` on
    /// `Base.new.x`.
    pub hook_singleton_installs: Vec<(String, (usize, usize))>,
    /// The same hook performed an installation whose NAME SET no AST here
    /// can read (`base.send(:define_method, ...)`, a string `class_eval`,
    /// a non-literal `delegate`) — the extender's instance surface may
    /// hold any name, so `apply_extended_hooks` opens it (fail-closed,
    /// invariant #1: silence, never a fabricated accusation).
    pub hook_installs_opaque: bool,
}

impl ClassFragment {
    fn new(path: String, is_module: bool, nesting: Vec<String>) -> Self {
        ClassFragment {
            path,
            nesting,
            is_module,
            superclass: None,
            includes: Vec::new(),
            prepends: Vec::new(),
            extends: Vec::new(),
            methods: Vec::new(),
            singleton_methods: Vec::new(),
            consts: Vec::new(),
            open: false,
            open_reason: None,
            table_name: None,
            mixes_in_class_methods: Vec::new(),
            declared_owner_required: false,
            hook_instance_installs: Vec::new(),
            hook_singleton_installs: Vec::new(),
            hook_installs_opaque: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileDefs {
    pub fragments: Vec<ClassFragment>,
    /// Malformed `#:` comments: (start, end, message) -> E0105.
    pub sig_errors: Vec<(usize, usize, String)>,
    /// This file monkeypatches a core class in a shape no fragment
    /// records (bead ita-2ve: top-level `include M`, `String.prepend(M)`,
    /// `Kernel.class_eval { def ... }`, top-level `def method_missing`).
    /// Forces the closed-world core lookup to stand down run-wide.
    pub core_mixin: bool,
    /// Every refinement target this file names, as `(target text minus
    /// any leading `::`, lexical nesting at the call site)` — e.g.
    /// `refine Integer do ... end` -> `[("Integer", [])]`. A refinement
    /// adds methods to a class through no fragment at all, so every
    /// closed-world core lookup on those classes must stand down. The
    /// nesting rides along because the target may be a constant ALIAS
    /// (`I = Integer; refine I`), and aliases can only be chased once
    /// every file is merged (`resolve_refined_core`).
    pub refine_targets: Vec<(String, Vec<String>)>,
    /// This file refines a target it could not name (`refine klass do`).
    /// Fail-closed: "some core class was refined, unknown which" stands
    /// every core class down, not none.
    pub refined_unknown: bool,
    /// Every class name this file hands a body no AST can read, as
    /// `(target text minus any leading `::`, lexical nesting at the call
    /// site)` — `Integer.class_eval("def +(o) = 'x'")` and the block form
    /// of the same call. Same contract and same two-phase alias
    /// resolution as `refine_targets` above, for the same reason: the
    /// methods arrive through no fragment at all. See `FileScan`'s
    /// `note_opaque_eval`.
    pub eval_targets: Vec<(String, Vec<String>)>,
    /// This file string-evals a body into a receiver it could not name
    /// (`klass.class_eval(str)`), or evals one anywhere at all
    /// (`eval(str)`, `Kernel.eval`, `binding.eval`). Fail-closed exactly
    /// like `refined_unknown`: "some class got a body we cannot read,
    /// unknown which" stands every core class down.
    pub eval_unknown: bool,
    /// Every pollution source this file aims at a class, WITH the method
    /// names it can define: `(target — `None` for a class the file
    /// cannot name — nesting at the site, what it can define)`. The
    /// blanket fields above answer "could anything have been added?" for
    /// the closed-world core lookup; this one answers "could `+` have
    /// been added?" for E0108, which is a different question with a
    /// measured different answer (`PollutionSource`,
    /// `resolve_keyed_pollution`, `Checker::core_ops_unpolluted`).
    pub keyed_pollution: Vec<(Option<String>, Vec<String>, PollutionSource)>,
    /// Bead ita-src: names this file defines as a class/module inside a
    /// STRING literal — generated Ruby source no parse of this file can
    /// see. See `OpenReason::StringSourceDefined`.
    pub string_source_consts: Vec<String>,
    /// Value constants assigned at class/module-body or toplevel level
    /// (w12 closure, E0104 did-you-mean): `(qualified name, span)` as
    /// written, e.g. `("Foo::Bar", span-of-BAR)` for `module Foo; Bar = 1`.
    /// Names only, no values — suggestions and `defined at` need exactly
    /// this and nothing more. Deliberately NOT folded into
    /// `ClassFragment::consts` (simple names, resolution-only, no spans):
    /// different lifetime, different consumer, never merged together.
    pub consts: Vec<(String, usize, usize)>,
    /// Distinct literal `require '<lib>'` targets in this file (W3
    /// require/autoload). Prism 1.9 has no `RequireNode` — `require 'x'` is
    /// a plain `CallNode` — so this is collected in the walk's `CallNode`
    /// arm. `require_relative`/dynamic requires are deliberately NOT
    /// collected: relative targets never define stdlib constants, and a
    /// non-literal require can name anything (invariant #1: silence).
    pub requires: Vec<String>,
    /// Bare `CONST = ...` written at true file toplevel — outside every
    /// class/module body (`frag_idx == None` in `DefWalker`'s
    /// `ConstantWriteNode` arm). Bead ita-exc defect B: distinct from
    /// `consts` above, which feeds ONLY the resolution-inert did-you-mean
    /// map — see `ProjectIndex::toplevel_consts` for why a top-level
    /// constant needs a bucket resolution can actually reach.
    pub toplevel_consts: Vec<String>,
    /// `(owner, simple)` for every constant PATH write (`A::B = ...`) in
    /// this file — bead ita-exc defect B's third shape. Resolved against
    /// the owner's fragment only once every project file's fragments are
    /// merged; see `resolve_qualified_const_writes`.
    pub qualified_writes: Vec<(String, String)>,
    /// Bead ita-54k: constant-alias write sites (`X = Y`, or qualified
    /// `A::B = C::D`) whose RHS is ITSELF a literal constant path —
    /// `(full LHS path as written, lexical nesting at the WRITE site,
    /// RHS path as written)`. See `ProjectIndex::const_aliases` for the
    /// resolution contract (suppression-only, chased from `const_exists`,
    /// never from `resolve_const`).
    pub const_aliases: Vec<(String, Vec<String>, String)>,
    /// Bead ita-o8l.1: literal-constant `include`/`extend`/`prepend`
    /// targets reached through a DYNAMIC receiver (present, not a
    /// constant path, not literal `self` — the exact shape bead ita-a8z's
    /// own `DynamicMixinScan` flagged, e.g.
    /// `builder_class.include(ActionMethods)` where `builder_class` is a
    /// local variable) anywhere in this file, INCLUDING inside method
    /// bodies — `DefWalker`'s own contour-limited walk never visits
    /// those, which is why this is collected by a SEPARATE full-tree
    /// scan (`FileScan`), not `DefWalker` itself. `(track, name
    /// as written, nesting chain at the call site)`: `name`/`nesting`
    /// are exactly what `ProjectIndex::resolve_const` needs to resolve
    /// the mixed-in module for real, which can only happen once every
    /// project file's fragments are merged (the module's own definition
    /// may live in a different file than the dynamic `include` call) —
    /// see `resolve_dynamic_mixin_targets`.
    pub dynamic_mixin_targets: Vec<(MixinTrack, String, Vec<String>)>,
    /// Every mixin call in this file whose RECEIVER could be named — see
    /// `AttributedMixinEdge`. Collected by `FileScan`'s full-tree
    /// traversal (the same one `dynamic_mixin_targets` rides), because
    /// the canonical shape lives inside a `def` body
    /// (`app_base.rb`'s `builder_class.include(ActionMethods)`), which
    /// `DefWalker`'s contour-limited walk never visits.
    pub attributed_mixin_edges: Vec<AttributedMixinEdge>,
    /// `def f; <const path or ternary of const paths>; end`, as
    /// `(method name, path as written, lexical nesting at the def)` —
    /// the Rails `get_builder_class` shape
    /// (`defined?(::AppBuilder) ? ::AppBuilder : Rails::AppBuilder`),
    /// whose return value is a class the caller then mixes into. Only a
    /// body that is EXACTLY that expression is recorded: a body with any
    /// other statement, or any other expression, contributes nothing
    /// (fail-closed — a wrong receiver attribution is exactly the
    /// receiver-blind silence bead ita-a8z measured).
    pub const_returning_methods: Vec<(String, String, Vec<String>)>,
    /// Bead B of onda 2: every `<call>.run_load_hooks(<literal symbol>,
    /// <literal constant path>)` in this file, as `(base path as written,
    /// lexical nesting at the call site)` — the base whose INSTANCE
    /// surface `apply_load_hook_openness` marks open. Collected by the
    /// full-tree `FileScan` (not `DefWalker`, whose contour-limited walk
    /// never visits a `def` body, which is where every rails call site
    /// lives).
    pub load_hook_bases: Vec<(String, Vec<String>)>,
}

/// Extract definitions from one file. Depends only on this file's text.
#[salsa::tracked]
pub fn file_defs(db: &dyn salsa::Database, file: SourceFile) -> FileDefs {
    parse_defs_text(file.text(db))
}

/// Pure parse, independent of salsa/`SourceFile` (mirrors `schema.rs`'s
/// `parse_schema_text`) — lets `declarations.rs` extract fragments from
/// embedded RBI text (bead ita-3gs) without needing a fake `SourceFile`
/// salsa input just to reuse this walk.
pub fn parse_defs_text(text: &str) -> FileDefs {
    let parse = ruby_prism::parse(text.as_bytes());
    let line_index = LineIndex::new(text);

    // `#:` comments by the 0-based line they END on, so a def on line L+1
    // picks up the sig comment on line L.
    let mut sig_comments: HashMap<u32, (usize, usize)> = HashMap::new();
    for comment in parse.comments() {
        let loc = comment.location();
        let ctext = text[loc.start_offset()..loc.end_offset()].trim_end();
        if let Some(after_marker) = ctext.strip_prefix("#:") {
            // RDoc visibility directives (`#:nodoc:`, `#:doc:`, `#:yields:`,
            // ...) glue a word directly onto `#:` with no separating space
            // or punctuation. No valid RBS sig form starts that way — every
            // real form's first non-space character after `#:` is `(`,
            // `[`, or `-` (of `->`); see rbs_comment.rs's grammar and its
            // `malformed` test (`"Integer -> String"` is rejected even by
            // the parser itself). Checked on the UNTRIMMED remainder so a
            // real sig attempt with a leading space (`#: String ->`) is
            // never touched — only literally glued, letter-first forms are
            // RDoc directives, not sigs (bead ita-ekg).
            let glued_word = after_marker.starts_with(|c: char| c.is_alphanumeric() || c == '_');
            if !glued_word {
                let (line, _) = line_index.line_col(text, loc.start_offset());
                sig_comments.insert(line, (loc.start_offset(), loc.start_offset() + ctext.len()));
            }
        }
    }

    let mut w = DefWalker {
        text,
        line_index: &line_index,
        sig_comments: &sig_comments,
        fragments: Vec::new(),
        sig_errors: Vec::new(),
        core_mixin: false,
        consts: Vec::new(),
        requires: Vec::new(),
        toplevel_consts: Vec::new(),
        qualified_writes: Vec::new(),
        const_aliases: Vec::new(),
        pending_sorbet_ret: None,
    };
    w.walk_body("", &[], false, &parse.node());
    // ONE file-wide traversal answering both "does this shape appear
    // ANYWHERE in this file" questions — dynamic mixins and refinements.
    let mut scan = FileScan {
        nesting: Vec::new(),
        targets: Vec::new(),
        refine_targets: Vec::new(),
        refined_unknown: false,
        eval_targets: Vec::new(),
        eval_unknown: false,
        keyed_pollution: Vec::new(),
        string_source_consts: Vec::new(),
        nested: 0,
        attributed_mixin_edges: Vec::new(),
        const_returning_methods: Vec::new(),
        load_hook_bases: Vec::new(),
        def_locals: Vec::new(),
    };
    scan.visit(&parse.node());
    FileDefs {
        fragments: w.fragments,
        sig_errors: w.sig_errors,
        core_mixin: w.core_mixin,
        refine_targets: scan.refine_targets,
        refined_unknown: scan.refined_unknown,
        eval_targets: scan.eval_targets,
        eval_unknown: scan.eval_unknown,
        keyed_pollution: scan.keyed_pollution,
        string_source_consts: scan.string_source_consts,
        consts: w.consts,
        requires: w.requires,
        toplevel_consts: w.toplevel_consts,
        qualified_writes: w.qualified_writes,
        const_aliases: w.const_aliases,
        dynamic_mixin_targets: scan.targets,
        attributed_mixin_edges: scan.attributed_mixin_edges,
        const_returning_methods: scan.const_returning_methods,
        load_hook_bases: scan.load_hook_bases,
    }
}

/// Bead ita-o8l.1: which method-lookup track a dynamically-mixed-in
/// module's OWN methods land on for the includer. `include`/`prepend`
/// put the module's instance methods on the includer's INSTANCE track
/// (`ProjectIndex::lookup_method`'s walk); `extend` puts them on the
/// includer's SINGLETON track — exactly like a static `extend` already
/// does in `lookup_singleton`'s own `class.extends` handling, which
/// reads the extended module's `methods` map (never `singleton_methods`)
/// onto the extender's singleton. `dynamic_mixin_covers` reads the same
/// `methods` map for both tracks for this reason — see that function.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MixinTrack {
    Instance,
    Singleton,
}

/// Where the RECEIVER of an attributed mixin call gets its value from —
/// the half `dynamic_mixin_targets` deliberately never collected, because
/// bead ita-a8z measured both receiver-keyed candidates it tried
/// (`Global`, `BuilderName`) silencing real errors project-wide.
///
/// This one is different in kind, not in degree: it is not a guess about
/// which class a receiver IS, it is a reading of the value the receiver
/// provably holds at that call site. `X.include(M)` names `X`; a local
/// assigned a constant path holds that constant; a local assigned a
/// receiverless project method holds that method's own return, and a
/// ternary of constant paths holds ONE of exactly two constants. Every
/// shape here is all-or-nothing — one unreadable element drops the whole
/// call, never a partial guess — and a receiver that is neither of these
/// collects nothing (fail-closed, invariant #1).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum MixinReceiver {
    /// The receiver is a literal constant path (`X.include(M)`), or the
    /// value a ternary of constant paths yields — resolved in `nesting`,
    /// the lexical scope the path was WRITTEN in (the call site for a
    /// literal receiver, the `def`'s own scope for a method's body).
    Path { path: String, nesting: Vec<String> },
    /// The receiver is a local (or a bare receiverless call) whose value
    /// is a project method's return — resolved in phase 2 against
    /// `ProjectIndex::const_returning_methods`, once every file is merged
    /// (`get_builder_class` lives in another file than the
    /// `builder_class.include(ActionMethods)` call site in rails).
    Call { method: String },
}

/// One mixin call whose receiver this scan could NAME, held until every
/// file is merged (`apply_attributed_mixin_edges`). Distinct from
/// `FileDefs::dynamic_mixin_targets` on purpose: those feed the
/// name-keyed, receiver-blind `dynamic_mixin_covers` softening, and
/// widening THAT set would widen a project-wide suppression. These feed
/// one receiver-keyed decision only — "this class is known to include a
/// module that answers every name" — see `OpenReason::MethodMissing`.
///
/// INSTANCE TRACK ONLY (`include`/`prepend`), and that is a decision, not
/// an omission. `X.extend(M)` lands `M` on X's SINGLETON, while the only
/// openness this index has (`ClassDef::open`) is read by BOTH lookups
/// (`lookup_own` and `lookup_singleton`) — so opening X for an `extend`
/// edge would silence every INSTANCE lookup on X, which `extend` never
/// justifies. Nothing measured asks for that widening: no fixture carries
/// an `extend` edge, and the three public corpora are byte-equal with the
/// arm gone. The receiver-blind `dynamic_mixin_singleton_targets` path
/// keeps covering the singleton case exactly as it did before this scan
/// existed; a singleton-keyed openness is roadmap, never a guess.
///
/// The MIRROR direction (`include` opening the class for singleton
/// lookups too) is deliberately not narrowed, and that is a measurement
/// rather than an oversight: this checker emits no singleton `NotFound`
/// for a project class AT ALL (probed 2026-09-19: `class X; end;
/// X.absent_name` reports nothing, because every project class's ancestry
/// carries an ancestor this index cannot close). An instance-track
/// openness therefore cannot mislead a singleton verdict today, and a
/// per-track reason would be a mechanism no fixture and no mutant could
/// distinguish. If singleton `NotFound` ever becomes reachable for
/// project classes, splitting the reason per track comes FIRST — see
/// `OpenReason::MethodMissing`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct AttributedMixinEdge {
    pub receiver: MixinReceiver,
    /// The mixin's literal constant argument, as written.
    pub module: String,
    /// Lexical nesting at the mixin call site, which is what resolves
    /// `module` (`ActionMethods` inside `module Rails` is
    /// `Rails::ActionMethods`).
    pub module_nesting: Vec<String>,
}

/// What ONE pollution source can add to the class it targets — the
/// name-keyed half of the core-pollution question (`FileDefs::keyed_pollution`).
///
/// The blanket question the closed-world core lookup asks ("could
/// ANYTHING have been added to this class?") and the question E0108 asks
/// ("could `+` or a coercion hook have been added to this class?") are
/// different questions, and the measurement that separates them is
/// recorded in `AGENTS.md`: across rails, mastodon and discourse, 248
/// methods are defined directly inside core-class reopenings and ZERO of
/// them is an operator or `coerce`/`to_str`/`to_int`. So every source is
/// collected WITH the names it can define, and a source whose names
/// cannot be read is `Opaque` — fail-closed, exactly as before.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PollutionSource {
    /// Exactly these method names, read off a body this checker can see.
    Names(Vec<String>),
    /// Every method of this module (`Integer.include M`, `class Integer;
    /// include M; end`), resolved project-wide once every file is merged
    /// — see `resolve_keyed_pollution`.
    Module(String),
    /// A body no AST here can read (a string eval, a dynamic
    /// `define_method`, a class-body block, an unrecognized macro): any
    /// method name at all.
    Opaque,
}

/// One full-file prism traversal answering every "does this shape appear
/// ANYWHERE in this file" question the position-keyed `DefWalker` cannot:
/// dynamic mixins (below) and refinements (`visit_call_node`'s `refine`
/// half). They share a traversal deliberately — a second `Visit` over
/// every file measured 7.87 ms against `check/project_index`'s 7.04 ms
/// ceiling, and a name comparison inside the existing one is free.
///
/// Bead ita-o8l.1 (replaces bead ita-a8z's measurement-only
/// `A8zCandidate`/`DynamicMixinScan`, both deleted): full-tree scan
/// (unlike `DefWalker`, which never visits inside a `def` body) for
/// `<dynamic-receiver>.include/extend/prepend(<literal constant>)` — the
/// `builder_class.include(ActionMethods)` shape (receiver present, not a
/// constant path, not literal `self`). ita-a8z measured and REJECTED
/// keying this on the RECEIVING class (`Global`: opens every class,
/// silenced 100% of one private corpus's real errors; `BuilderName`:
/// opens every class named `*Builder`, measured to silence a real,
/// unrelated E0101 three files away). This scan instead keys on the
/// method NAME at the lookup site — see `ProjectIndex::
/// dynamic_mixin_covers` — so it never opens any class at all; it only
/// ever collects candidate MODULES (the literal argument), tracked with
/// the lexical nesting in effect at the call site so
/// `ProjectIndex::resolve_const` can resolve them for real once every
/// file is merged (`resolve_dynamic_mixin_targets`). Always runs — no
/// env-gate — because it can only ever soften an existing `NotFound`
/// into `Inconclusive` (invariant #1: strictly less diagnostic, never
/// more), unlike ita-a8z's two rejected candidates, which could and did
/// hide real errors.
struct FileScan {
    /// Real `Module.nesting` chain in effect at the current point in the
    /// walk, outermost first — same discipline as `DefWalker`'s own
    /// `nesting` (bead ita-519): one entry per `class`/`module` keyword,
    /// never per `::`-segment, and `class << self` pushes nothing.
    nesting: Vec<String>,
    targets: Vec<(MixinTrack, String, Vec<String>)>,
    /// Every refinement target this file names, with the nesting it was
    /// written in — see `FileDefs::refine_targets` and the `refine` half
    /// of `visit_call_node`.
    refine_targets: Vec<(String, Vec<String>)>,
    /// A refinement whose target could not be named — see
    /// `FileDefs::refined_unknown`.
    refined_unknown: bool,
    /// Every core-class-pollution target from an unreadable eval body —
    /// see `FileDefs::eval_targets` and `note_opaque_eval`.
    eval_targets: Vec<(String, Vec<String>)>,
    /// An eval body whose target could not be named — see
    /// `FileDefs::eval_unknown`.
    eval_unknown: bool,
    /// Name-keyed pollution: every source this file aims at a class,
    /// `(target, nesting at the site, what it can define)`. `None` as a
    /// target means the source hit a class this file cannot name, so it
    /// counts against EVERY class — with `Opaque` that is the old
    /// project-wide stand-down, and with `Names` it is bounded to those
    /// names. See `FileDefs::keyed_pollution`.
    keyed_pollution: Vec<(Option<String>, Vec<String>, PollutionSource)>,
    /// Bead ita-src: every `class X`/`module X` name written inside a
    /// STRING literal in this file — Ruby source the project generates
    /// at runtime and this parse never sees.
    string_source_consts: Vec<String>,
    /// How many `def`/block bodies deep the walk currently is. Zero plus
    /// an empty `nesting` is TRUE file toplevel — the only place a bare
    /// `include M` really lands on `Object` and a bare `def` really
    /// defines an `Object` method. Without this the scan read every
    /// `RSpec.describe do include Foo end` as an `Object` mixin and
    /// stood `Object` down on all three public corpora (measured: 81
    /// phantom `Object` methods on mastodon, plus an `Opaque` from the
    /// first unresolvable spec helper), which is exactly the strictness
    /// the blanket collector never had.
    nested: usize,
    /// Mixin calls with a namable receiver — see `AttributedMixinEdge`.
    attributed_mixin_edges: Vec<AttributedMixinEdge>,
    /// `def f; <const path / ternary of const paths>; end` — see
    /// `FileDefs::const_returning_methods`.
    const_returning_methods: Vec<(String, String, Vec<String>)>,
    /// `run_load_hooks(:sym, <literal base>)` bases, with the lexical
    /// nesting at the call site — bead B of onda 2, see
    /// `FileDefs::load_hook_bases` and `apply_load_hook_openness`.
    load_hook_bases: Vec<(String, Vec<String>)>,
    /// One frame per `def` body currently being walked, mapping a local
    /// variable's name to the receivers its last written value can be.
    /// A local is what carries the value from `get_builder_class` to
    /// `builder_class.include(...)`; without the frame the mixin call's
    /// receiver is just "some unknown object" and the edge stays
    /// unattributed (today's behavior, and the fail-closed default).
    /// Only writes whose value is one of `MixinReceiver`'s readable
    /// shapes are recorded, and a later unreadable write to the same
    /// name OVERWRITES the entry with nothing — a local reassigned to
    /// something else must not keep resolving to its old value.
    def_locals: Vec<FxHashMap<String, Vec<MixinReceiver>>>,
}

impl FileScan {
    /// Push one nesting level for `path_node` (a class/module's own
    /// `constant_path()`), scoped under whatever is already on the
    /// stack — mirrors `DefWalker`'s `join_path(scope, &path)` exactly.
    /// `None` (a dynamic class/module path, e.g. `class self.class::X`)
    /// pushes nothing: vanishingly rare, and any dynamic-mixin call
    /// found inside just keeps resolving against the OUTER nesting
    /// instead — a false negative, never a false positive.
    fn push_scope(&mut self, path_node: &Node<'_>) {
        if let Some(path) = const_path_str(path_node) {
            let scope = self.nesting.last().map_or("", String::as_str);
            self.nesting.push(join_path(scope, &path));
        }
    }

    fn pop_scope(&mut self, path_node: &Node<'_>) {
        if const_path_str(path_node).is_some() {
            self.nesting.pop();
        }
    }
}

impl<'pr> Visit<'pr> for FileScan {
    fn visit_class_node(&mut self, node: &ruby_prism::ClassNode<'pr>) {
        let path = node.constant_path();
        self.push_scope(&path);
        ruby_prism::visit_class_node(self, node);
        self.pop_scope(&path);
    }

    fn visit_module_node(&mut self, node: &ruby_prism::ModuleNode<'pr>) {
        let path = node.constant_path();
        self.push_scope(&path);
        ruby_prism::visit_module_node(self, node);
        self.pop_scope(&path);
    }

    fn visit_def_node(&mut self, node: &ruby_prism::DefNode<'pr>) {
        // A top-level `def method_missing` lands on `Object`, so EVERY
        // receiver — core included — dispatches through it (bead ita-2ve
        // condition (b), where `DefWalker` flags it project-wide). Keyed
        // by name here: it is exactly `method_missing` on `Object`, and
        // E0108 asks after that name specifically.
        if self.nesting.is_empty() && self.nested == 0 && node.receiver().is_none() {
            let name = String::from_utf8_lossy(node.name().as_slice()).into_owned();
            if name == "method_missing" || name == "respond_to_missing?" {
                self.keyed_pollution.push((
                    Some("Object".to_string()),
                    Vec::new(),
                    PollutionSource::Names(vec![name]),
                ));
            }
        }
        self.note_const_returning_body(node);
        self.nested += 1;
        self.def_locals.push(FxHashMap::default());
        ruby_prism::visit_def_node(self, node);
        self.def_locals.pop();
        self.nested -= 1;
    }

    fn visit_block_node(&mut self, node: &ruby_prism::BlockNode<'pr>) {
        self.nested += 1;
        ruby_prism::visit_block_node(self, node);
        self.nested -= 1;
    }

    fn visit_local_variable_write_node(&mut self, node: &ruby_prism::LocalVariableWriteNode<'pr>) {
        // The value a local holds is what decides whose class the mixin
        // call below targets. `remove`, never `insert`: this write
        // REPLACES whatever the name held, so an unreadable value must
        // clear the entry rather than leave a stale one.
        let name = String::from_utf8_lossy(node.name().as_slice()).into_owned();
        let value = node.value();
        let candidates = self.value_receivers(&value);
        if let Some(frame) = self.def_locals.last_mut() {
            if candidates.is_empty() {
                frame.remove(&name);
            } else {
                frame.insert(name, candidates);
            }
        }
        ruby_prism::visit_local_variable_write_node(self, node);
    }

    fn visit_string_node(&mut self, node: &ruby_prism::StringNode<'pr>) {
        collect_string_source_consts(
            &String::from_utf8_lossy(node.unescaped()),
            &mut self.string_source_consts,
        );
        ruby_prism::visit_string_node(self, node);
    }

    fn visit_call_node(&mut self, node: &ruby_prism::CallNode<'pr>) {
        self.note_dynamic_mixin(node);
        self.note_attributed_mixin(node);
        self.note_refinement(node);
        self.note_opaque_eval(node);
        self.note_injection(node);
        self.note_load_hook_base(node);
        ruby_prism::visit_call_node(self, node);
    }
}

impl FileScan {
    /// `<dynamic-receiver>.include/extend/prepend(<literal constant>)` —
    /// see this struct's doc comment for why the RECEIVER is never the
    /// key.
    fn note_dynamic_mixin(&mut self, node: &ruby_prism::CallNode<'_>) {
        let Some(track) = mixin_track(node.name().as_slice()) else { return };
        let dynamic_receiver = node
            .receiver()
            .is_some_and(|recv| const_path_str(&recv).is_none() && recv.as_self_node().is_none());
        if !dynamic_receiver {
            return;
        }
        let Some(args) = node.arguments() else { return };
        for arg in &args.arguments() {
            if let Some(name) = const_path_str(&arg) {
                self.targets.push((track, name, self.nesting.clone()));
            }
        }
    }

    /// `<namable receiver>.include/extend/prepend(<literal constant>)` —
    /// see `AttributedMixinEdge`. Implicit-`self` receivers are
    /// `DefWalker`'s own arm (the enclosing class's real ancestry edge),
    /// and a receiver whose value this scan cannot read collects
    /// nothing: the mixin still softens by method NAME project-wide
    /// through `dynamic_mixin_covers`, exactly as before.
    fn note_attributed_mixin(&mut self, node: &ruby_prism::CallNode<'_>) {
        // `MixinTrack::Singleton` is deliberately not attributed — see
        // `AttributedMixinEdge`'s doc comment: the openness it would set is
        // read by both lookups, so an `extend` edge would silence instance
        // lookups that `extend` does not justify.
        match mixin_track(node.name().as_slice()) {
            Some(MixinTrack::Instance) => {}
            _ => return,
        }
        let Some(recv) = node.receiver() else { return };
        if recv.as_self_node().is_some() {
            return;
        }
        let Some(args) = node.arguments() else { return };
        let modules: Vec<String> = args.arguments().iter().filter_map(|a| const_path_str(&a)).collect();
        // All-or-nothing: `include M, whatever` names a module set this
        // scan cannot enumerate, so it attributes none of them.
        if modules.is_empty() || modules.len() != args.arguments().len() {
            return;
        }
        let receivers = self.receiver_of(&recv);
        if receivers.is_empty() {
            return;
        }
        for receiver in receivers {
            for module in &modules {
                self.attributed_mixin_edges.push(AttributedMixinEdge {
                    receiver: receiver.clone(),
                    module: module.clone(),
                    module_nesting: self.nesting.clone(),
                });
            }
        }
    }

    /// What class this receiver expression provably names, if any — the
    /// three readable shapes `MixinReceiver` documents. Anything else
    /// (a chained call, an ivar, a literal) yields nothing.
    fn receiver_of(&self, recv: &Node<'_>) -> Vec<MixinReceiver> {
        if let Some(read) = recv.as_local_variable_read_node() {
            let name = String::from_utf8_lossy(read.name().as_slice()).into_owned();
            return self
                .def_locals
                .last()
                .and_then(|frame| frame.get(&name))
                .cloned()
                .unwrap_or_default();
        }
        self.value_receivers(recv)
    }

    /// What class the VALUE of `node` provably names. A constant path is
    /// itself; a ternary whose two arms are constant paths is one of
    /// exactly those two (the predicate is never consulted — whatever it
    /// evaluates to, the value is one of the arms); a bare receiverless
    /// project call is deferred to phase 2 (`MixinReceiver::Call`),
    /// because its body lives in another file. A bare constant read is
    /// `Call`-free here only when it is a `LocalVariableReadNode`, which
    /// `receiver_of` handles — a `CallNode` with no receiver and no
    /// arguments is a method call, and that is the `get_builder_class`
    /// spelling.
    fn value_receivers(&self, node: &Node<'_>) -> Vec<MixinReceiver> {
        if let Some(path) = const_path_str(node) {
            return vec![MixinReceiver::Path { path, nesting: self.nesting.clone() }];
        }
        if let Some(ternary) = ternary_const_paths(node) {
            return ternary
                .into_iter()
                .map(|path| MixinReceiver::Path { path, nesting: self.nesting.clone() })
                .collect();
        }
        if let Some(call) = node.as_call_node() {
            if call.receiver().is_none()
                && call.arguments().is_none()
                && call.block().is_none()
                && !call.name().as_slice().is_empty()
            {
                return vec![MixinReceiver::Call {
                    method: String::from_utf8_lossy(call.name().as_slice()).into_owned(),
                }];
            }
        }
        Vec::new()
    }

    /// `def f; <a constant path, or a ternary of two constant paths>; end`
    /// — recorded so a caller holding `f`'s return can name the receiver
    /// of its mixin call. Only a body that IS that expression once,
    /// alone, qualifies; any other statement or expression contributes
    /// nothing (see `FileDefs::const_returning_methods`).
    fn note_const_returning_body(&mut self, node: &ruby_prism::DefNode<'_>) {
        if node.receiver().is_some() {
            return;
        }
        let Some(body) = node.body() else { return };
        let Some(only) = sole_statement(&body) else { return };
        let paths = ternary_const_paths(&only)
            .or_else(|| const_path_str(&only).map(|p| vec![p]))
            .unwrap_or_default();
        if paths.is_empty() {
            return;
        }
        let name = String::from_utf8_lossy(node.name().as_slice()).into_owned();
        for path in paths {
            self.const_returning_methods.push((name.clone(), path, self.nesting.clone()));
        }
    }

    /// A refinement (`refine Integer do def +(o) ... end end`) adds
    /// methods to a class through neither a class fragment (`by_path`)
    /// nor a method-injection call (`core_mixin`), so without this the
    /// closed-world core lookup stays conclusive on a class whose
    /// operator has been replaced, and E0108 accuses `1 + "s"` on a
    /// program MRI runs clean (measured: `using` a module that refines
    /// `Integer#+` prints `"refined s"`, exit 0).
    ///
    /// Collected in the file-wide scan, and not as an arm in
    /// `DefWalker::walk_body`, which is round 4's whole point: the body
    /// walker only reaches class/module-body and toplevel statements, so
    /// three shapes MRI runs clean were still accused — `refine` inside
    /// `def self.install` (a method body), inside `Module.new do ...
    /// end` (a block), and inside `SomeMod.module_eval do ... end`. This
    /// visit recurses into defs, blocks and conditionals, so every one
    /// of them is seen. It rides the traversal the dynamic-mixin scan
    /// already does: a SECOND `Visit` over every file was measured at
    /// 112% of the `check/project_index` perf ceiling (7.87 ms vs
    /// 7.04 ms), and a name comparison inside the existing one is free.
    ///
    /// The mark is project-wide and ignores `using`'s lexical scope
    /// entirely: tracking which files a refinement is activated in buys
    /// only false positives if the tracking is ever wrong, and a
    /// refinement of a core class is rare enough that the false negative
    /// costs nothing measurable (zero `refine` calls in all four
    /// corpora).
    ///
    /// Requiring a block keeps a project method merely NAMED `refine`
    /// from poisoning anything unless it also takes one. A block
    /// ARGUMENT (`refine Integer, &blk`) counts: it is free, since MRI
    /// rejects that form outright with `ArgumentError: can't pass a Proc
    /// as a block to Module#refine`.
    fn note_refinement(&mut self, node: &ruby_prism::CallNode<'_>) {
        if node.name().as_slice() != b"refine" || node.block().is_none() {
            return;
        }
        // The name-keyed half: a refinement body is ordinary readable
        // Ruby, so what it can define is exactly what `block_pollution`
        // reads off it — `refine Integer do def zz ... end end` cannot
        // change `+`, which E0108 now proves instead of assuming.
        let target = node
            .arguments()
            .and_then(|a| a.arguments().iter().next())
            .and_then(|a| const_path_str(&a))
            .map(|p| p.trim_start_matches("::").to_string());
        let sources = block_pollution(node.block().as_ref());
        self.push_keyed(target.as_deref(), sources);
        match node
            .arguments()
            .and_then(|a| a.arguments().iter().next())
            .and_then(|a| const_path_str(&a))
        {
            // `::Integer` and `Integer` are the same class; a project
            // constant (`Foo`, `Foo::Bar`) is kept as written, so it
            // matches no core name and poisons nothing. A target that
            // cannot be named at all (`refine klass do`) is fail-closed:
            // "some core class was refined and we do not know which" is
            // exactly the state in which no core receiver can be proven,
            // so it stands EVERY core class down rather than none.
            Some(path) => self
                .refine_targets
                .push((path.trim_start_matches("::").to_string(), self.nesting.clone())),
            None => self.refined_unknown = true,
        }
    }

    /// `Integer.class_eval("def +(o) = 'x'")` replaces a core operator
    /// through a body no AST can read, exactly like `refine` adds one
    /// through no fragment: measured on ruby 3.4.2, that line followed by
    /// `p 1 + "s"` prints `"evaled"` and exits 0, while E0108 accused it.
    ///
    /// The toplevel/class-body contour of the literal-receiver shape was
    /// already covered by `core_injection_call` (which sets the
    /// project-wide `core_mixin`). What reaches here is every contour
    /// that walk never visits — a method body, a block, a conditional —
    /// plus the two shapes a literal-receiver test can never see: a
    /// receiver spelled as a constant ALIAS, and a receiver that cannot
    /// be named at all. All eleven were measured as live false positives
    /// before this arm existed (`ita check` accusing, `ruby` exit 0); the
    /// fixtures are `crates/itaruby_semantic/tests/operand_types.rs`'s
    /// eval section, each one executed by MRI in the same table as the
    /// refinement sources.
    ///
    /// What counts as a body nobody showed us:
    ///
    /// - any positional argument to `class_eval`/`module_eval`/
    ///   `instance_eval`: those three take a STRING and nothing else, so
    ///   an argument of any shape (literal, heredoc, interpolated, or a
    ///   variable) is a body. `instance_eval` is in the list because
    ///   `define_method` inside it defines an INSTANCE method —
    ///   `Integer.instance_eval("define_method(:+) { |o| 'ie' }")` really
    ///   prints `"ie"` (measured), so "`instance_eval` only touches the
    ///   singleton" is false;
    /// - a block on any of those, or on `class_exec`/`module_exec`/
    ///   `instance_exec`. A block IS walkable, and reading its defs
    ///   instead would be more precise — but the walker-reachable contour
    ///   already treats the block form as pollution whatever its body
    ///   (`core_injection_call` lists `class_eval` with no look at the
    ///   argument), and one program must not mean two things depending on
    ///   whether it sits in a method body or at toplevel. The cost is a
    ///   measured false negative kept identical to the toplevel one: a
    ///   block that defines something else
    ///   (`Integer.class_eval { def doubled = self * 2 }`) stops accusing
    ///   `1 + "s"` (see `a_block_eval_body_that_defines_nothing_relevant_is_a_false_negative`).
    ///
    /// The polluted class is the RECEIVER: a constant path as written
    /// (chased through aliases in `resolve_eval_polluted_core`), or the
    /// innermost lexical nesting for `self.class_eval`/receiverless
    /// `class_eval` — the Rails `class_eval <<~RUBY` idiom, whose
    /// implicit receiver is the enclosing class. That fallback is what
    /// keeps the idiom from escalating to the project-wide mark below,
    /// and a project class matches no core name and poisons nothing.
    ///
    /// A receiver that cannot be named plus a STRING body
    /// (`klass.class_eval(str)`, `Object.const_get(x).class_eval(str)`)
    /// is the `refined_unknown` decision again: "some class got a body we
    /// cannot read, and we do not know which class" is exactly the state
    /// in which no core receiver is provable, so it stands EVERY core
    /// class down rather than none. The same unnamable receiver with a
    /// BLOCK deliberately does not: `x.instance_eval { ... }` is ordinary
    /// DSL code in every Ruby project (25 such sites in rails, 16 in
    /// discourse), and marking on a shape that names no class at all
    /// would turn E0108 off everywhere for free.
    ///
    /// `eval`/`Kernel.eval`/`binding.eval` with any argument stands every
    /// core class down for the same reason: the string can define
    /// anything anywhere, and `eval("class Integer; def +(o) = 'x'; end")`
    /// followed by `p 1 + "s"` is a program MRI runs clean. E0108's
    /// locals map does NOT already cover this (measured both ways):
    /// `OperandLocalScan::visit_call_node` bails the scope it is scanning,
    /// which is why `x = 1; eval("x = 'str'"); x + 1` is silent — but two
    /// LITERAL operands need no local at all, and
    /// `eval(...); p 1 + "s"` accused. The price is real and accepted,
    /// and it is smaller than it first looked: measured with prism over
    /// the three public corpora, rails has 62 `eval` sites with an
    /// argument and discourse 120, so both carry this mark — but neither
    /// project had E0108 alive BEFORE this arm existed either (probe:
    /// a `p 1 + "s"` file added to each clone, checked with the parent
    /// revision's binary — 0 E0108 rows on all three corpora), because
    /// `core_mixin` and a project reopening of `Object`/`Kernel` already
    /// stood every core class down. Only mastodon has no unknown mark at
    /// all, and its `1 + "s"` is blocked by a project reopening of
    /// `Numeric`, not by any eval.
    ///
    /// So parsing a LITERAL eval body as a sub-program and marking only
    /// what it really touches — the obvious next step — was measured and
    /// is NOT worth building: of the sites that stand every core class
    /// down, only 4 of 98 in rails and 46 of 123 in discourse have a
    /// literal body at all, and both projects keep dozens of genuinely
    /// dynamic ones (`eval(local)`, `eval(call)`, an interpolated
    /// `class_eval <<-CODE`), so the project-wide mark survives either
    /// way and no corpus changes verdict. See AGENTS.md's
    /// `Removed — do not reintroduce`. What WOULD move the capability is
    /// name-keyed pollution (ask whether a reopening can touch
    /// `+ - * /` or a coercion hook): across all three corpora, 248
    /// methods are defined directly in core-class reopenings and ZERO of
    /// them is an operator or `coerce`/`to_str`/`to_int`.
    fn note_opaque_eval(&mut self, node: &ruby_prism::CallNode<'_>) {
        let id = node.name();
        let name = id.as_slice();
        let has_arg = node
            .arguments()
            .is_some_and(|a| a.arguments().iter().next().is_some());
        if name == b"eval" {
            self.eval_unknown |= has_arg;
            // Unchanged policy, now expressed in the keyed collection
            // too: a bare `eval(<string>)` can define anything anywhere,
            // so it stands every class down for every name. Whether that
            // is the right price for a RUNTIME string is the one switch
            // this change deliberately leaves to the owner (CHANGELOG).
            if has_arg {
                self.push_keyed(None, vec![PollutionSource::Opaque]);
            }
            return;
        }
        if !matches!(
            name,
            b"class_eval"
                | b"module_eval"
                | b"instance_eval"
                | b"class_exec"
                | b"module_exec"
                | b"instance_exec"
        ) {
            return;
        }
        let string_body = has_arg && matches!(name, b"class_eval" | b"module_eval" | b"instance_eval");
        if !string_body && node.block().is_none() {
            return;
        }
        let target = self.eval_target(node);
        match &target {
            Some(path) => self
                .eval_targets
                .push((path.trim_start_matches("::").to_string(), self.nesting.clone())),
            None if string_body => self.eval_unknown = true,
            None => {}
        }
        let keyed_target = target.map(|p| p.trim_start_matches("::").to_string());
        self.note_eval_keyed(node, keyed_target.as_deref(), string_body);
    }

    /// Which class an eval-family call runs its body in: the receiver if
    /// it can be named, the enclosing class for a receiverless or `self`
    /// receiver (the Rails `class_eval <<~RUBY` idiom), `None` for a
    /// receiver this file cannot name.
    fn eval_target(&self, node: &ruby_prism::CallNode<'_>) -> Option<String> {
        let Some(recv) = node.receiver() else {
            return self.nesting.last().cloned();
        };
        if recv.as_self_node().is_some() {
            return self.nesting.last().cloned();
        }
        const_path_str(&recv)
    }

    /// The name-keyed half of `note_opaque_eval`.
    ///
    /// A BLOCK body is readable Ruby, so it contributes exactly the names
    /// `block_pollution` reads off it. A STRING body stays `Opaque`:
    /// parsing a literal eval body as a sub-program was measured against
    /// all three public corpora and rejected (AGENTS.md, `Removed — do
    /// not reintroduce`), so "this class got a body we cannot read" is
    /// still the honest answer, now scoped to the class it names instead
    /// of all of them.
    fn note_eval_keyed(
        &mut self,
        node: &ruby_prism::CallNode<'_>,
        target: Option<&str>,
        string_body: bool,
    ) {
        if string_body {
            // The string case keeps the shipped contract exactly: a body
            // no AST can read stands its class down, and an unnamable
            // receiver stands every class down.
            self.push_keyed(target, vec![PollutionSource::Opaque]);
            return;
        }
        // A BLOCK case with an unnamable receiver records NOTHING, and
        // that is a deliberate limit rather than an oversight: the
        // shipped blanket collector recorded nothing there either
        // (`note_opaque_eval`'s `None => {}` arm), and treating it as
        // fail-closed instead stood every core class down on mastodon —
        // `obj.instance_exec(&blk)` is everywhere in real code, and the
        // probe measured E0108 dead project-wide because of it. The gap
        // it leaves is the pre-existing one: a block body evaled into a
        // receiver nobody can name.
        if target.is_none() {
            return;
        }
        let sources = block_pollution(node.block().as_ref());
        self.push_keyed(target, sources);
    }

    /// Record one source against `target` — `None` meaning "a class this
    /// file cannot name", which counts against every class (see
    /// `FileScan::keyed_pollution`). Empty `sources` is the common case
    /// and records nothing: a body that defines no method pollutes no
    /// name.
    fn push_keyed(&mut self, target: Option<&str>, sources: Vec<PollutionSource>) {
        let nesting = self.nesting.clone();
        for src in sources {
            self.keyed_pollution
                .push((target.map(ToString::to_string), nesting.clone(), src));
        }
    }

    /// `run_load_hooks(:sym, <literal base>)` — bead B of onda 2.
    ///
    /// `ActiveSupport.on_load(:sym) { ... }` registers a block, and
    /// `run_load_hooks(:sym, Base)` hands `Base` to it; the library's own
    /// `execute_hook` then runs `base.class_eval(&block)` when the base
    /// is a Module (`activesupport/lib/active_support/lazy_load_hooks.rb:
    /// 107`). The block and the base are decoupled by a SYMBOL and the
    /// hook's receiver is a method PARAMETER, so no per-`def` attribution
    /// can reach the installed methods: the base's real instance surface
    /// is exactly `class_eval`'s output, which is not in any project
    /// fragment. Marking the named base `open` is therefore the only
    /// correct answer — `Inconclusive`, never `NotFound` (`NotFound` was
    /// measured wrong on rails' `LazyLoadHooksTest::FakeContext`, whose
    /// three E0101 lines are this shape; `testdata/lazy_load/`).
    ///
    /// Deliberately narrow, all-or-nothing:
    /// * exactly TWO positional arguments, the first a LITERAL symbol and
    ///   the second a literal constant path. `run_load_hooks(:x)` alone
    ///   defaults its base to `Object` and would open every receiver in
    ///   the project; a base that is `self`, a local, or a call is not a
    ///   name this pass can resolve, and an unresolved name contributes
    ///   nothing (`apply_load_hook_openness`).
    /// * the receiver is unconstrained, exactly as `core_injection_call`'s
    ///   family is: `ActiveSupport.run_load_hooks(...)`, a bare
    ///   `run_load_hooks(...)` inside the library's own
    ///   `extend LazyLoadHooks`, and a chained receiver all name the same
    ///   public API, and a project callback named `run_load_hooks` that
    ///   takes a literal symbol and a constant would mean the same thing
    ///   anyway.
    fn note_load_hook_base(&mut self, node: &ruby_prism::CallNode<'_>) {
        if node.name().as_slice() != b"run_load_hooks" {
            return;
        }
        let Some(args) = node.arguments() else { return };
        let args = args.arguments();
        if args.len() != 2 || args.iter().next().and_then(|a| a.as_symbol_node()).is_none() {
            return;
        }
        let Some(base) = args.iter().nth(1).and_then(|a| const_path_str(&a)) else { return };
        self.load_hook_bases.push((base, self.nesting.clone()));
    }

    /// `Integer.include M` / `String.define_method(:+) { }` /
    /// `Hash.send(:alias_method, :x, :y)` / top-level `include M` — every
    /// injection shape `core_injection_call` flags project-wide, now
    /// carrying the names it can actually inject.
    ///
    /// The eval family is deliberately NOT handled here:
    /// `note_opaque_eval` above already owns those shapes, including
    /// their receiver resolution and their string/block split.
    fn note_injection(&mut self, node: &ruby_prism::CallNode<'_>) {
        // Cheap name test before anything allocates: this runs on EVERY
        // call node in every file, and `definer_sources` collects the
        // argument list into a `Vec` (measured: the allocation, not the
        // work, is what a per-call collector costs at corpus scale).
        if !is_definer_name(node.name().as_slice()) {
            return;
        }
        let sources = definer_sources(node);
        if sources.is_empty() {
            return;
        }
        let Some(target) = self.injection_target(node) else {
            return;
        };
        self.push_keyed(Some(target.as_str()), sources);
    }

    /// Which class an injection call adds to, or `None` for the shapes
    /// this collector deliberately does not record.
    ///
    /// A bare `include M` at true top level mixes into `Object`; inside a
    /// class/module body it is that body's own business, and the merged
    /// fragment already carries it. Bare `extend`/`prepend` at top level
    /// only touch `main`'s singleton — not a core patch, exactly as
    /// `DefWalker` has it.
    ///
    /// A receiver this file cannot name (`klass.include M`) is not
    /// recorded either: `core_injection_call`'s project-wide mark
    /// requires a constant receiver too, so poisoning here would make
    /// the keyed question STRICTER than the blanket one it refines — and
    /// the shape is everywhere (50 sites in rails alone), so it would
    /// stand every core class down on every real project. The gap it
    /// leaves is the pre-existing one `dynamic_mixin_covers` softens for
    /// E0101.
    fn injection_target(&self, node: &ruby_prism::CallNode<'_>) -> Option<String> {
        let Some(recv) = node.receiver() else {
            let bare_toplevel_include = node.name().as_slice() == b"include"
                && self.nesting.is_empty()
                && self.nested == 0;
            return bare_toplevel_include.then(|| "Object".to_string());
        };
        if recv.as_self_node().is_some() {
            return self.nesting.last().cloned();
        }
        const_path_str(&recv).map(|p| p.trim_start_matches("::").to_string())
    }

}

/// What a block body can define — the `refine`/`class_eval { }`
/// source. A block with no body defines nothing.
fn block_pollution(block: Option<&ruby_prism::Node<'_>>) -> Vec<PollutionSource> {
    let Some(block) = block else { return Vec::new() };
    let Some(block) = block.as_block_node() else {
        // A block ARGUMENT (`refine Integer, &blk`,
        // `class_eval(&blk)`): a body this file does not contain, so
        // `Opaque` rather than "defines nothing". Measured on ruby
        // 3.4.2, the `refine` spelling of it raises `ArgumentError:
        // can't pass a Proc as a block to Module#refine` before any
        // operand runs, so this arm costs a false negative only on
        // code that already crashes — and `class_eval(&blk)`, which is
        // legal, needs exactly this.
        return vec![PollutionSource::Opaque];
    };
    let body = block.body();
    statements_pollution(body.as_ref(), 0)
}

/// Every method name a class body / block body can define, or
/// `Opaque` for each statement whose effect cannot be read.
///
/// Fail-closed by construction: a `def` and a recognized definer
/// macro contribute NAMES, anything else that could run code at
/// definition time — an unrecognized receiverless macro, a block, a
/// dynamic `define_method` — contributes `Opaque`. Measured against
/// the three public corpora before it was written: the only
/// receiverless calls that appear inside a core reopening there are
/// `alias_method` (12), `include` (3), `private` (2) and
/// `module_function` (2), so reading exactly those plus `def` costs
/// nothing real and the `Opaque` fallback stays honest.
fn statements_pollution(body: Option<&ruby_prism::Node<'_>>, depth: usize) -> Vec<PollutionSource> {
    let mut out = Vec::new();
    let Some(body) = body else { return out };
    // A conditional/`begin` wrapper is transparent; deeper than this
    // is not worth a walk, and `Opaque` is the fail-closed answer.
    if depth > 3 {
        out.push(PollutionSource::Opaque);
        return out;
    }
    if let Some(stmts) = body.as_statements_node() {
        for st in &stmts.body() {
            out.extend(statement_pollution(&st, depth));
        }
        return out;
    }
    out.extend(statement_pollution(body, depth));
    out
}

/// One statement of a class body / block body.
fn statement_pollution(st: &Node<'_>, depth: usize) -> Vec<PollutionSource> {
    if let Some(def) = st.as_def_node() {
        let name = String::from_utf8_lossy(def.name().as_slice()).into_owned();
        return vec![PollutionSource::Names(vec![name])];
    }
    if let Some(al) = st.as_alias_method_node() {
        return match literal_method_name(&al.new_name()) {
            Some(n) => vec![PollutionSource::Names(vec![n])],
            None => vec![PollutionSource::Opaque],
        };
    }
    if let Some(call) = st.as_call_node() {
        return call_statement_pollution(&call, depth);
    }
    // `class << self`, `if`/`unless`/`else`, `begin`: transparent
    // wrappers around more statements.
    nested_statements(st)
        .iter()
        .flat_map(|inner| statements_pollution(Some(inner), depth + 1))
        .collect()
}

/// The statement lists a wrapper statement holds — the only nodes whose
/// children can still define a method on the body's own class. Anything
/// else (a constant write, an assignment, a nested class or module with
/// its own target) contributes nothing and yields an empty list here.
fn nested_statements<'a>(st: &Node<'a>) -> Vec<Node<'a>> {
    if let Some(sc) = st.as_singleton_class_node() {
        return sc.body().into_iter().collect();
    }
    if let Some(n) = st.as_if_node() {
        let then = n.statements().map(|s| s.as_node());
        return then.into_iter().chain(n.subsequent()).collect();
    }
    if let Some(n) = st.as_unless_node() {
        let then = n.statements().map(|s| s.as_node());
        let other = n.else_clause().map(|e| e.as_node());
        return then.into_iter().chain(other).collect();
    }
    if let Some(n) = st.as_else_node() {
        return n.statements().map(|s| s.as_node()).into_iter().collect();
    }
    if let Some(n) = st.as_begin_node() {
        return n.statements().map(|s| s.as_node()).into_iter().collect();
    }
    Vec::new()
}

/// One statement-position call inside a class/refinement body.
fn call_statement_pollution(call: &ruby_prism::CallNode<'_>, depth: usize) -> Vec<PollutionSource> {
    let mut out = Vec::new();
    // `private def foo` / `module_function def bar`: the visibility
    // macro is inert, the `def` in its arguments is not.
    if let Some(args) = call.arguments() {
        for arg in &args.arguments() {
            if let Some(def) = arg.as_def_node() {
                out.push(PollutionSource::Names(vec![String::from_utf8_lossy(
                    def.name().as_slice(),
                )
                .into_owned()]));
            }
        }
    }
    let definers = definer_sources(call);
    if !definers.is_empty() {
        out.extend(definers);
        return out;
    }
    if call.receiver().is_some() {
        // An explicit receiver defines on THAT object, not on the
        // body's own class — and the eval family is `note_opaque_eval`'s.
        return out;
    }
    if call.block().is_some() {
        // `FIELDS.each { define_method ... }`, `included do ... end`:
        // exactly `OpenReason::ClassBodyBlock`, unreadable.
        out.push(PollutionSource::Opaque);
        return out;
    }
    let name = call.name();
    if !out.is_empty() || INERT_BODY_CALLS.contains(&name.as_slice()) {
        return out;
    }
    // An unrecognized receiverless macro in a core-class body can be
    // a definer this checker has never heard of.
    let _ = depth;
    out.push(PollutionSource::Opaque);
    out
}

/// Receiverless calls that appear in class bodies and define no method:
/// visibility and bookkeeping. Everything not listed is `Opaque` in a
/// core-class body — see `FileScan::call_statement_pollution`.
const INERT_BODY_CALLS: &[&[u8]] = &[
    b"private",
    b"public",
    b"protected",
    b"module_function",
    b"private_class_method",
    b"public_class_method",
    b"private_constant",
    b"public_constant",
    b"require",
    b"require_relative",
    b"freeze",
    b"raise",
    b"puts",
    b"warn",
    b"using",
];

/// The names one INJECTION call can define, whatever its receiver:
/// `define_method`/`alias_method`/`attr_*`/`delegate` with literal names
/// are read; `include`/`prepend`/`extend` carry the whole module;
/// `send(:define_method, ...)` unwraps one level; a dynamic name is
/// `Opaque`. An empty result means "this call defines nothing", which is
/// every other call in the language.
/// Could a call with this name define a method? The one-line filter in
/// front of `definer_sources`, which must agree with its match arms —
/// every name below has an arm there, and nothing else does.
fn is_definer_name(name: &[u8]) -> bool {
    matches!(
        name,
        b"define_method"
            | b"define_singleton_method"
            | b"alias_method"
            | b"attr_accessor"
            | b"attr_reader"
            | b"attr_writer"
            | b"attr"
            | b"include"
            | b"prepend"
            | b"extend"
            | b"delegate"
            | b"delegate_missing_to"
            | b"def_delegator"
            | b"def_delegators"
            | b"def_instance_delegator"
            | b"send"
            | b"public_send"
            | b"__send__"
    )
}

fn definer_sources(call: &ruby_prism::CallNode<'_>) -> Vec<PollutionSource> {
    let id = call.name();
    let name = id.as_slice();
    let args: Vec<Node<'_>> = call
        .arguments()
        .map(|a| a.arguments().iter().collect())
        .unwrap_or_default();
    definer_sources_named(name, &args)
}

/// `definer_sources` on an already-unwrapped `(name, args)` pair, so the
/// `send(:define_method, :+)` shape can reuse every rule exactly.
fn definer_sources_named(name: &[u8], args: &[Node<'_>]) -> Vec<PollutionSource> {
    let one = |i: usize| match args.get(i).and_then(literal_method_name) {
        Some(m) => vec![PollutionSource::Names(vec![m])],
        None => vec![PollutionSource::Opaque],
    };
    match name {
        b"define_method" | b"define_singleton_method" | b"alias_method" => one(0),
        b"attr_accessor" | b"attr_reader" | b"attr_writer" | b"attr" => attr_sources(name, args),
        b"include" | b"prepend" | b"extend" => mixin_sources(args),
        // `delegate :foo, :bar, to: :baz` defines `foo`/`bar`;
        // `delegate_missing_to` installs `method_missing` itself.
        b"delegate" => delegate_sources(args),
        b"delegate_missing_to" => vec![PollutionSource::Names(vec![
            "method_missing".to_string(),
            "respond_to_missing?".to_string(),
        ])],
        // Forwardable: readable in principle, unread here on purpose —
        // zero sites in the corpora, and `Opaque` is one class, not all.
        b"def_delegator" | b"def_delegators" | b"def_instance_delegator" => {
            vec![PollutionSource::Opaque]
        }
        // `Integer.send(:define_method, :+) { }`: unwrap one level and
        // every rule above applies unchanged.
        b"send" | b"public_send" | b"__send__" => match args.first().and_then(literal_method_name) {
            Some(inner) => definer_sources_named(inner.as_bytes(), &args[1..]),
            None => vec![PollutionSource::Opaque],
        },
        _ => Vec::new(),
    }
}

/// `attr_accessor :a, :b` -> `a`, `a=`, `b`, `b=`; `attr_reader` drops
/// the writers, `attr_writer` the readers. One non-literal argument makes
/// the whole call unreadable.
fn attr_sources(name: &[u8], args: &[Node<'_>]) -> Vec<PollutionSource> {
    let mut names = Vec::new();
    for arg in args {
        let Some(m) = literal_method_name(arg) else {
            return vec![PollutionSource::Opaque];
        };
        if name != b"attr_writer" {
            names.push(m.clone());
        }
        if matches!(name, b"attr_accessor" | b"attr_writer") {
            names.push(format!("{m}="));
        }
    }
    if names.is_empty() {
        return Vec::new();
    }
    vec![PollutionSource::Names(names)]
}

/// `include A, B` -> one `Module` source each; a non-constant argument is
/// a method set nobody can read.
fn mixin_sources(args: &[Node<'_>]) -> Vec<PollutionSource> {
    args.iter()
        .map(|arg| match const_path_str(arg) {
            Some(p) => PollutionSource::Module(p.trim_start_matches("::").to_string()),
            None => PollutionSource::Opaque,
        })
        .collect()
}

/// `delegate :foo, :bar, to: :baz` — the literal names, ignoring the
/// keyword hash, and `Opaque` on anything else.
fn delegate_sources(args: &[Node<'_>]) -> Vec<PollutionSource> {
    let mut names = Vec::new();
    for arg in args {
        match literal_method_name(arg) {
            Some(m) => names.push(m),
            // The `to:`/`prefix:` keyword hash is not a name.
            None if arg.as_keyword_hash_node().is_some() => {}
            None => return vec![PollutionSource::Opaque],
        }
    }
    if names.is_empty() {
        return Vec::new();
    }
    vec![PollutionSource::Names(names)]
}

struct DefWalker<'a> {
    text: &'a str,
    line_index: &'a LineIndex,
    sig_comments: &'a HashMap<u32, (usize, usize)>,
    fragments: Vec<ClassFragment>,
    sig_errors: Vec<(usize, usize, String)>,
    /// Set when this file monkeypatches a core class without creating a
    /// fragment (bead ita-2ve) — see `core_injection_call` and the
    /// top-level `include`/`method_missing` arms below.
    core_mixin: bool,
    /// Qualified value-constant assignments (w12 closure) — see
    /// `FileDefs::consts`.
    consts: Vec<(String, usize, usize)>,
    /// Distinct literal `require '<lib>'` targets in this file (W3) —
    /// see `FileDefs::requires`.
    requires: Vec<String>,
    /// Bare `CONST = ...` written at true file toplevel — see
    /// `FileDefs::toplevel_consts`.
    toplevel_consts: Vec<String>,
    /// `(owner, simple)` for every constant PATH write — see
    /// `FileDefs::qualified_writes`.
    qualified_writes: Vec<(String, String)>,
    /// See `FileDefs::const_aliases`.
    const_aliases: Vec<(String, Vec<String>, String)>,
    /// Raw `.returns(...)` text of the most recently walked `sig { ... }`
    /// statement (bead ita-uh1), threaded from one class-body statement
    /// to the immediately next one by `walk_stmts` so the `def` that
    /// follows a `sig` can pick it up in `method_def`. Cleared before any
    /// statement that is neither a `sig` call nor a `def` — only
    /// direct-adjacency counts, matching the contract's "def vier
    /// precedido de um sig".
    pending_sorbet_ret: Option<String>,
}

impl DefWalker<'_> {
    /// Walk statements that form the body of a class/module (or toplevel,
    /// scope == ""). `in_singleton` is true inside `class << self`.
    /// `nesting` is the REAL `Module.nesting` chain in effect INSIDE this
    /// body (already includes this class/module itself as the innermost
    /// entry — bead ita-519); the caller computes it, never derived here
    /// by splitting `scope` on `::` (that's exactly the bug this bead
    /// fixes).
    fn walk_body(&mut self, scope: &str, nesting: &[String], in_singleton: bool, node: &Node<'_>) {
        // Fragment for this scope: created lazily; multiple `walk_body`
        // calls for the same scope produce multiple fragments (reopening),
        // merged by `project_index`.
        let frag_idx = if scope.is_empty() {
            None
        } else {
            self.fragments
                .push(ClassFragment::new(scope.to_string(), false, nesting.to_vec()));
            Some(self.fragments.len() - 1)
        };
        self.walk_stmts(scope, nesting, frag_idx, in_singleton, node);
    }

    /// Mark fragment `i` open for `reason` (bead ita-anc). First-reason-wins
    /// with ONE precedence (2026-09-03, discourse `ImportScripts::Base`
    /// measured): `AbstractRaise` is the WEAKEST reason — it is the only
    /// one instance lookups may pass through — so a later, stronger open
    /// on the same fragment (a class-body delegate loop, an eval, a
    /// `method_missing`) must REPLACE it. Plain first-reason-wins let a
    /// class that opens with a raise stub and later loops `%i[...].each {
    /// delegate ... }` masquerade as a pure abstract stub.
    fn open_class(&mut self, i: usize, reason: OpenReason) {
        let f = &mut self.fragments[i];
        f.open = true;
        if f.open_reason.is_none()
            || (f.open_reason == Some(OpenReason::AbstractRaise)
                && reason != OpenReason::AbstractRaise)
        {
            f.open_reason = Some(reason);
        }
    }

    /// The method list a definition inside fragment `i` belongs to.
    /// `in_singleton` is true inside a `class << self` body, where every
    /// method-defining form — `def`, `attr_*`, `define_method`,
    /// `alias_method`, `alias` — lands on the CLASS OBJECT and on
    /// nothing else (singleton-track family (a); MRI agrees, see
    /// `sclass_define_method_is_not_an_instance_method.rb`).
    fn track(&mut self, i: usize, in_singleton: bool) -> &mut Vec<MethodDef> {
        let f = &mut self.fragments[i];
        if in_singleton {
            &mut f.singleton_methods
        } else {
            &mut f.methods
        }
    }

    /// File one literal definition found inside a `def` body, or fail
    /// closed. Three cases, and the decision rule is whether `self` at
    /// that point is PROVABLY the class:
    ///
    /// * enclosing target in a `def self.x` (or `class << self`) body —
    ///   `self` IS the class, so the call really defines the class's
    ///   methods: file them, `define_singleton_method` on the
    ///   class-object track and the rest on the instance track. This is
    ///   the case discourse's `GlobalSetting` is made of, and the
    ///   fixture `def_body_literal_definers_resolve_silently.rb` is the
    ///   MRI proof that all four spellings really land there.
    /// * enclosing target in an INSTANCE method body — `self` is one
    ///   object. `define_singleton_method` there widens that object's
    ///   surface and says nothing about the class, and every other
    ///   definer here is a `NoMethodError` on a non-Module receiver. The
    ///   attribution is not provable, so the class OPENS rather than
    ///   collecting names it may not have (invariant #1).
    /// * a literal constant receiver (`Other.define_method(:x)`) — that
    ///   class's surface changes whenever this `def` runs, and the
    ///   enclosing class learns nothing. Open THAT fragment, exactly as
    ///   the dynamic case already does, and never register onto the
    ///   enclosing one. Filing onto a foreign fragment would have to go
    ///   through `apply_singleton_patches`' declared-owner rule, which
    ///   is a separate decision from this one.
    fn apply_body_def(
        &mut self,
        i: usize,
        nesting: &[String],
        self_is_the_class: bool,
        span: (usize, usize),
        target: DefTarget,
        lit: BodyDefLiteral,
    ) {
        let DefTarget::Enclosing = target else {
            if let DefTarget::Named(path) = target {
                let oi = self.fragment_idx_for(&path, nesting);
                self.open_class(oi, OpenReason::DynamicDefineMethod);
            }
            return;
        };
        if !self_is_the_class {
            self.open_class(i, OpenReason::DynamicDefineMethod);
            return;
        }
        let mut md = MethodDef::synthetic(lit.name, lit.required, span);
        md.arity_unknown = lit.arity_unknown;
        self.track(i, lit.singleton).push(md);
    }

    /// `%w(template copy_file ...).each do |method| class_eval <<-RUBY
    /// def #{method}(...); @generator.send(:#{method}, ...); end RUBY end`
    /// — a literal list crossed with an interpolated string body defines
    /// exactly those names, and nothing in this walker can see them
    /// otherwise (`class_eval` is a call with a string argument, and a
    /// `def` written inside a string is not an AST node at all).
    ///
    /// This is `rails/railties/lib/rails/generators/rails/app/
    /// app_generator.rb`'s `Rails::ActionMethods`: nine forwarding
    /// methods, invisible to any per-`def`-node scan.
    ///
    /// What it is worth, measured honestly (re-measured 2026-09-19): in
    /// the configuration that ships, this harvest's MARGINAL contribution
    /// to the three public corpora is ZERO — rails lands on the same 6
    /// E0101 with it and without it, and mastodon and discourse are
    /// byte-identical. The "95 sites" an earlier version of this comment
    /// cited was this harvest measured WITHOUT the const-returning-ternary
    /// arm, which is not a configuration that ships, so it was not
    /// evidence for keeping it. It is kept for the shape its own fixture
    /// proves against runtime ground truth — a method-PARAMETER include
    /// receiver, where every receiver-keyed mechanism attributes nothing
    /// and this is the only thing that sees the names at all — and the
    /// cost it carries is named rather than hidden: the names land on the
    /// module's own map, so they soften through the pre-existing
    /// NAME-keyed, receiver-blind `dynamic_mixin_covers`, exactly like
    /// every other name that module defines.
    ///
    /// Naming the abandonment, because it looks like a contradiction:
    /// "parse the literal body of an eval" was BUILT and ABANDONED on
    /// 2026-09-18 for the E0108 pollution mark, and the reason recorded
    /// there was that no corpus observes a difference — every core-class
    /// eval body in three public corpora defines names no operator
    /// question ever asks about. That reason does NOT carry here: this
    /// harvest is the ONLY way those nine names become visible at all,
    /// and its own fixture is a runtime-proven false positive without it.
    /// The abandoned decision stays abandoned for its own question.
    ///
    /// Deliberately shallow and all-or-nothing: one `each` whose
    /// receiver is a literal array of strings/symbols, one block
    /// parameter, an `eval`-family call whose first argument is an
    /// interpolated string that interpolates THAT parameter and nothing
    /// else, and a substituted body that parses with zero prism errors.
    /// Any other shape contributes nothing (fail-closed).
    fn harvest_interpolated_eval_defs(&mut self, i: usize, call: &ruby_prism::CallNode<'_>) {
        if call.name().as_slice() != b"each" {
            return;
        }
        let Some(array) = call.receiver().and_then(|r| r.as_array_node()) else { return };
        let Some(block) = call.block().and_then(|b| b.as_block_node()) else { return };
        let Some(param) = sole_block_param(&block) else { return };
        let elements = literal_string_elements(&array);
        if elements.is_empty() {
            return;
        }
        let Some(body) = block.body() else { return };
        for template in eval_string_templates(&body, &param) {
            // The body runs in the class the eval call NAMES, and the
            // names belong there. A template naming ANOTHER class is
            // skipped rather than filed on the enclosing one: filing it
            // here would both invent a method this class never gains and
            // leave the class that really gains it unfiled — and it would
            // feed `dynamic_mixin_covers` a name the enclosing class's
            // mixers do not have. Filing on the resolved target instead
            // needs `fragment_idx_for` plus the declared-owner rule that
            // `apply_body_def` documents as a separate decision, so it is
            // roadmap here, never a guess (fail-closed, invariant #1).
            if let Some(target) = &template.explicit_target {
                if *target != self.fragments[i].path {
                    continue;
                }
            }
            let placeholder = format!("#{{{param}}}");
            for element in &elements {
                let source = template.text.replace(&placeholder, element);
                for name in top_level_def_names(&source) {
                    let mut md = MethodDef::synthetic(name, 0, template.span);
                    md.arity_unknown = true;
                    self.fragments[i].methods.push(md);
                }
            }
        }
    }

    /// Defect A (bead ita-exc): literal constant writes directly inside a
    /// class-body call's block — `enums do; Alpha = new(...); end`, or
    /// the fixture's invented `constvis_enums do; ... end` (any method
    /// name, never a hardcoded list; see the call site below). Constant
    /// *definition* is lexical: `instance_eval`/`instance_exec`/
    /// `class_eval` rebind `self`, never the cref, so a literal
    /// `CONST = ...` written straight in such a block still defines the
    /// constant on the LEXICALLY ENCLOSING class `i` — exactly the same
    /// self/cref asymmetry bead ita-sit recorded from the other
    /// direction. A block that never runs at all simply never defines
    /// the constant at runtime: a false negative, which invariant #1
    /// permits.
    ///
    /// Deliberately shallow, mirroring the `define_method` carve-out
    /// above rather than a general traversal: only the block's own
    /// top-level statements are inspected. `def`s, nested classes,
    /// conditionals, and anything else in the block body are left alone
    /// by this pass — recursing into those would risk the same
    /// false-E0101 exposure a broader traversal already cost bead
    /// ita-d0j. A qualified write (`A::B = ...`) found here is routed
    /// through the same `qualified_writes` merge-time resolution as a
    /// top-level one (defect B) rather than blindly attributed to `i` —
    /// it may not even name a constant on THIS class.
    fn harvest_block_consts(&mut self, i: usize, block: &Node<'_>) {
        let Some(b) = block.as_block_node() else { return };
        let Some(body) = b.body() else { return };
        let Some(stmts) = body.as_statements_node() else { return };
        for stmt in &stmts.body() {
            if let Some(cw) = stmt.as_constant_write_node() {
                let name = String::from_utf8_lossy(cw.name().as_slice()).into_owned();
                self.fragments[i].consts.push(name);
            } else if let Some(cpw) = stmt.as_constant_path_write_node() {
                if let Some(path) = const_path_str(&cpw.target().as_node()) {
                    let full = path.trim_start_matches("::");
                    if let Some((owner, simple)) = full.rsplit_once("::") {
                        self.qualified_writes.push((owner.to_string(), simple.to_string()));
                    }
                }
            }
        }
    }

    /// Bead ita-blk: every `class X`/`module X` KEYWORD anywhere inside a
    /// class-body block, registered at the path Ruby's LEXICAL cref gives
    /// it and marked `OpenReason::BlockNestedDefinition`.
    ///
    /// A `class` keyword never takes its cref from the block's runtime
    /// `self` — `Foo.class_eval { class Bar; end }` defines `Bar` in the
    /// block's lexical scope, not under `Foo` — so the path computed here
    /// is the one MRI uses, and the recursion keeps that true for a
    /// definition nested inside another.
    ///
    /// FAIL-CLOSED, never a surface: the fragment carries no methods and
    /// is born OPEN. The bug it fixes is receiver IDENTITY, not receiver
    /// surface: without the fragment, `Foo.config` inside
    /// `RailtiesTest::RailtieTest` resolved against whatever unrelated
    /// top-level `Foo` the project happened to contain and read as a
    /// conclusive `NotFound` (4 of rails' 33 residue records, all of them
    /// code that runs). With it, the name resolves to the class the code
    /// really defines, whose body this walk has not read — `Inconclusive`,
    /// which is the truth.
    fn harvest_block_nested_definitions(
        &mut self,
        scope: &str,
        nesting: &[String],
        block: &Node<'_>,
    ) {
        let Some(b) = block.as_block_node() else { return };
        let Some(body) = b.body() else { return };
        let mut out: Vec<(String, Vec<String>, bool)> = Vec::new();
        {
            let mut scan = NestedDefScan {
                scope: vec![scope.to_string()],
                nesting: nesting.to_vec(),
                out: &mut out,
            };
            scan.visit(&body);
        }
        for (path, nest, is_module) in out {
            let mut frag = ClassFragment::new(path, is_module, nest);
            frag.open = true;
            frag.open_reason = Some(OpenReason::BlockNestedDefinition);
            self.fragments.push(frag);
        }
    }

    /// `class_methods do ... end` — the block spelling of a concern's
    /// `ClassMethods` module (singleton-track family (c)). The block's
    /// own top-level `def`s are recorded on a synthetic fragment for
    /// `<concern>::ClassMethods`, and the concern gains the `extends`
    /// edge to it, so this spelling resolves through exactly the
    /// mechanism `apply_concern_class_methods` gives the written-out
    /// module. Deliberately shallow, the same discipline
    /// `harvest_block_consts` and `harvest_included_hook` follow: only
    /// top-level `def`s, never a nested traversal.
    fn harvest_class_methods_block(
        &mut self,
        i: usize,
        scope: &str,
        nesting: &[String],
        call: &ruby_prism::CallNode,
    ) {
        let Some(block) = call.block().and_then(|b| b.as_block_node()) else { return };
        let Some(stmts) = block.body().and_then(|b| b.as_statements_node()) else { return };
        let path = format!("{scope}::ClassMethods");
        let mut defs: Vec<MethodDef> = Vec::new();
        for stmt in &stmts.body() {
            let Some(def) = stmt.as_def_node() else { continue };
            if def.receiver().is_some() {
                continue;
            }
            defs.push(self.method_def(&def));
        }
        if defs.is_empty() {
            return;
        }
        if !self.fragments[i].extends.contains(&path) {
            self.fragments[i].extends.push(path.clone());
        }
        let mut child_nesting = nesting.to_vec();
        child_nesting.push(path.clone());
        let idx = if let Some(idx) = self.fragments.iter().position(|f| f.path == path) {
            idx
        } else {
            self.fragments.push(ClassFragment::new(path, true, child_nesting));
            self.fragments.len() - 1
        };
        self.fragments[idx].methods.extend(defs);
    }

    /// Find, or create, the fragment for a class/module path this walker
    /// is not lexically inside — the shape
    /// `X.singleton_class.prepend M` and `class << X` both need, since
    /// each names its target by constant path from somewhere else
    /// entirely (discourse's `spec/support/discourse_event_helper.rb`
    /// patches `DiscourseEvent` from a file that never opens it).
    ///
    /// The fragment is created as a CLASS (`is_module: false`) only when
    /// it does not exist: a real `class`/`module` keyword elsewhere in
    /// the project merges into the same `by_path` entry and its own
    /// `is_module` wins, because `merge_file_fragments` keeps the first
    /// real declaration's shape. Nothing here invents a name the source
    /// did not write.
    fn fragment_idx_for(&mut self, path: &str, nesting: &[String]) -> usize {
        if let Some(idx) = self.fragments.iter().position(|f| f.path == path) {
            return idx;
        }
        let mut child_nesting = nesting.to_vec();
        child_nesting.push(path.to_string());
        let mut frag = ClassFragment::new(path.to_string(), false, child_nesting);
        // Not written by a keyword here: only `apply_singleton_patches`
        // may merge it, and only onto a path the project declares.
        frag.declared_owner_required = true;
        self.fragments.push(frag);
        self.fragments.len() - 1
    }


    /// Bead ita-1yw: the canonical hook shape `def self.included(base)`
    /// whose body calls `base.attr_accessor :x` / `base.define_method(:x)`
    /// defines instance methods on every includer — none of which this
    /// walker otherwise sees (def bodies are never walked). Only the
    /// literal shape is modeled: exactly one required positional parameter
    /// (no optionals/rest/posts/keywords/kwrest/block), and only top-level
    /// body statements whose receiver is a bare read of THAT parameter
    /// (shallow scan, same discipline as `harvest_block_consts`). attr_*
    /// registers all-or-nothing (a single non-literal argument skips the
    /// whole call — never a partial guess); `define_method` registers only
    /// via `literal_method_name`. Discovered names land on the module's own
    /// fragment, so `include Mod -> ancestors -> Mod.methods` resolves them
    /// with zero new lookup mechanism. Deliberately NO `open_class`
    /// fallback: a hook doing anything else (the common `base.class_eval`
    /// string form, `send`, dynamic names) simply registers nothing — the
    /// includer's real methods stay a documented false negative
    /// (invariant #1), never a false positive.
    fn harvest_included_hook(&mut self, i: usize, def: &ruby_prism::DefNode) {
        let Some(pname) = Self::hook_receiver_param_name(def) else { return };
        let Some(body) = def.body() else { return };
        let Some(stmts) = body.as_statements_node() else { return };
        for stmt in &stmts.body() {
            let Some(call) = stmt.as_call_node() else { continue };
            let Some(recv) = call.receiver() else { continue };
            let Some(read) = recv.as_local_variable_read_node() else { continue };
            if String::from_utf8_lossy(read.name().as_slice()) != pname {
                continue;
            }
            let name = String::from_utf8_lossy(call.name().as_slice()).into_owned();
            match name.as_str() {
                "attr_reader" | "attr_writer" | "attr_accessor" => {
                    self.harvest_hook_attr(i, &name, &call);
                }
                "define_method" => {
                    if let Some(m) = call
                        .arguments()
                        .and_then(|a| a.arguments().iter().next())
                        .and_then(|a| literal_method_name(&a))
                    {
                        let mut md = MethodDef::synthetic(m, 0, span_of(&stmt));
                        md.arity_unknown = true;
                        self.fragments[i].methods.push(md);
                    }
                }
                _ => {}
            }
        }
    }

    /// attr_* arm of `harvest_included_hook` — all-or-nothing: a single
    /// non-literal argument skips the whole call, never a partial guess.
    fn harvest_hook_attr(&mut self, i: usize, name: &str, call: &ruby_prism::CallNode) {
        let mut attrs: Vec<(String, (usize, usize))> = Vec::new();
        if let Some(args) = call.arguments() {
            for arg in &args.arguments() {
                let Some(sym) = arg.as_symbol_node() else { return };
                attrs.push((
                    String::from_utf8_lossy(sym.unescaped()).into_owned(),
                    span_of(&arg),
                ));
            }
        }
        for (attr, aspan) in attrs {
            if name != "attr_writer" {
                self.fragments[i]
                    .methods
                    .push(MethodDef::synthetic(attr.clone(), 0, aspan));
            }
            if name != "attr_reader" {
                self.fragments[i]
                    .methods
                    .push(MethodDef::synthetic(format!("{attr}="), 1, aspan));
            }
        }
    }


    /// The hook's receiver parameter name when `def` is exactly the literal
    /// `def self.included(base)` shape — one required positional parameter
    /// and nothing else (no optionals/rest/posts/keywords/kwrest/block).
    fn hook_receiver_param_name(def: &ruby_prism::DefNode) -> Option<String> {
        let params = def.parameters()?;
        let mut requireds = params.requireds().iter();
        let param = requireds.next()?;
        if requireds.next().is_some()
            || params.optionals().iter().next().is_some()
            || params.rest().is_some()
            || params.posts().iter().next().is_some()
            || params.keywords().iter().next().is_some()
            || params.keyword_rest().is_some()
            || params.block().is_some()
        {
            return None;
        }
        let rp = param.as_required_parameter_node()?;
        Some(String::from_utf8_lossy(rp.name().as_slice()).into_owned())
    }

    /// Bead H of onda 2: `def self.extended(base)` — what that hook
    /// installs on `base` is what every `extend`er of this module really
    /// answers to, and no fragment of this project holds it.
    ///
    /// The canonical shape is `ActiveModel::Naming`'s:
    ///
    /// ```ruby
    /// def self.extended(base)
    ///   base.silence_redefinition_of_method :model_name
    ///   base.delegate :model_name, to: :class
    /// end
    /// ```
    ///
    /// `Blog::Post` does `extend ActiveModel::Naming`
    /// (`activemodel/test/models/blog_post.rb:9`), and `delegate` installs
    /// `model_name` as an INSTANCE method of `Blog::Post` — so
    /// `Blog::Post.new.model_name`, code that runs, was one of rails'
    /// baseline errors (`activemodel/test/cases/naming_test.rb:333`).
    /// `extend M` on its own reads `M`'s own instance methods onto the
    /// extender's SINGLETON track (`lookup_singleton`), which is exactly
    /// why the install this hook performs on the extender's INSTANCE
    /// surface was invisible here.
    ///
    /// Four install shapes are read, and only these (the same shallow,
    /// literal-only discipline `harvest_included_hook` uses — no second
    /// traversal, no block chasing):
    /// * `base.delegate :a, :b, <keywords>` — the positional symbols.
    /// * `base.define_method(:x)` / `base.define_method("x")`.
    /// * `base.class_eval do ... end` with a LITERAL block: that block
    ///   runs with `self` = base, so its bare `def`s and receiverless
    ///   `define_method` calls are base's own instance methods.
    /// * `def base.x` — filed on the extender's SINGLETON surface, where
    ///   it really lands; `Base.new.x` must keep accusing.
    ///
    /// Every other statement contributes NOTHING (`base.tag_stack = ...`,
    /// `base.instance_variable_set(...)`, `base.const_set(...)`, an
    /// unknown macro): a documented false negative, exactly like the
    /// `included` hook's own unmodeled shapes. The one exception is the
    /// fail-closed arm — a call that provably INSTALLS but whose NAME SET
    /// cannot be read marks the hook opaque, and `apply_extended_hooks`
    /// opens the extender's instance surface (invariant #1 prefers silence
    /// to a fabricated accusation).
    fn harvest_extended_hook(&mut self, i: usize, def: &ruby_prism::DefNode) {
        let Some(pname) = Self::hook_receiver_param_name(def) else { return };
        let Some(body) = def.body() else { return };
        let Some(stmts) = body.as_statements_node() else { return };
        let mut consumed = 0usize;
        for stmt in &stmts.body() {
            consumed += usize::from(self.harvest_extended_stmt(i, &pname, &stmt));
        }
        // Bead ita-esc, the fail-closed half of the shallow walk: every
        // read of `base` the shallow pass did NOT consume is `base`
        // escaping into code this harvest never reads, so the installs
        // cannot be enumerated. discourse's `Migrations::Enum` is the
        // measured shape — its `self.extended(base)` body is one
        // statement, `TracePoint.new(:end) do |tp| ... end.enable`, and
        // every `base.define_singleton_method(...)` lives INSIDE that
        // block, where the top-level scan cannot see it. Before this,
        // such a hook contributed nothing at all and the extender read
        // as a complete, closed surface (4 of discourse's 20 residue
        // records). Silence-only: `apply_extended_hooks` turns the flag
        // into `OpenReason::EvalOrSend` on the extender.
        if count_local_reads(&body, &pname) > consumed {
            self.fragments[i].hook_installs_opaque = true;
        }
    }

    /// One top-level statement of a `self.extended(base)` body — see
    /// `harvest_extended_hook` for the shapes that count. Returns whether
    /// this statement CONSUMED a read of the hook parameter as its own
    /// receiver, which is how `harvest_extended_hook` tells a modelled
    /// install from `base` escaping somewhere it cannot read.
    fn harvest_extended_stmt(&mut self, i: usize, pname: &str, stmt: &Node<'_>) -> bool {
        if let Some(def) = stmt.as_def_node() {
            // `def base.x` installs on the base OBJECT's singleton, never
            // on its instances: filing it as an instance method would
            // silence the real `NoMethodError` that `Base.new.x` raises.
            if def.receiver().is_some_and(|r| hook_param_read(&r, pname)) {
                let name = String::from_utf8_lossy(def.name().as_slice()).into_owned();
                self.fragments[i].hook_singleton_installs.push((name, span_of(stmt)));
                return true;
            }
            return false;
        }
        let Some(call) = stmt.as_call_node() else { return false };
        let Some(recv) = call.receiver() else { return false };
        if hook_param_read(&recv, pname) {
            self.harvest_extended_call(i, &call);
            return true;
        }
        false
    }

    /// The install shapes a `self.extended(base)` body can carry on the
    /// base itself; the caller has already proved the receiver is the
    /// hook's own parameter.
    fn harvest_extended_call(&mut self, i: usize, call: &ruby_prism::CallNode<'_>) {
        let name = String::from_utf8_lossy(call.name().as_slice()).into_owned();
        match name.as_str() {
            "delegate" => self.harvest_hook_delegate(i, call),
            "define_method" => self.harvest_hook_define(i, call, false),
            // Bead ita-dsm: the SINGLETON spelling of the same install —
            // `base.define_singleton_method(:values) { ... }` puts the name
            // on the extender's class object, the very track `extend`
            // dispatches on. discourse's `Migrations::Enum` installs
            // `valid?`/`values` this way (four census residue records were
            // conclusive misses on them).
            "define_singleton_method" => self.harvest_hook_define(i, call, true),
            "class_eval" | "module_eval" | "class_exec" | "module_exec" => {
                self.harvest_extended_eval_block(i, call);
            }
            "send" | "public_send" | "__send__" | "instance_eval" | "instance_exec" => {
                self.fragments[i].hook_installs_opaque = true;
            }
            _ => {}
        }
    }

    /// `extend M`'s `delegate :a, :b, to: ...` files each named method on
    /// the extender's instance surface; an unreadable form makes the whole
    /// install opaque. Split from `harvest_extended_call` for the ceiling.
    fn harvest_hook_delegate(&mut self, i: usize, call: &ruby_prism::CallNode<'_>) {
        match hook_delegate_names(call) {
            Some(names) => {
                for (n, span) in names {
                    self.fragments[i].hook_instance_installs.push((n, span));
                }
            }
            None => self.fragments[i].hook_installs_opaque = true,
        }
    }

    /// `define_method`/`define_singleton_method` in a hook body: a literal
    /// name is filed (on the singleton surface when `singleton`, else the
    /// instance surface); a non-literal name makes the install opaque.
    fn harvest_hook_define(
        &mut self,
        i: usize,
        call: &ruby_prism::CallNode<'_>,
        singleton: bool,
    ) {
        match hook_define_method_name(call) {
            Some(n) => {
                let loc = call.location();
                let span = (loc.start_offset(), loc.end_offset());
                if singleton {
                    self.fragments[i].hook_singleton_installs.push((n, span));
                } else {
                    self.fragments[i].hook_instance_installs.push((n, span));
                }
            }
            None => self.fragments[i].hook_installs_opaque = true,
        }
    }

    /// `base.class_eval do ... end`: the block runs with `self` = base, so
    /// the `def`s written directly in it are base's own instance methods.
    /// Any argument at all (a string body, a `&proc`) is a body no AST
    /// here can read; so is a nested installer whose names are unknowable
    /// (`send`-family, a non-literal `define_method`).
    fn harvest_extended_eval_block(&mut self, i: usize, call: &ruby_prism::CallNode<'_>) {
        if call.arguments().is_some() {
            self.fragments[i].hook_installs_opaque = true;
            return;
        }
        let Some(block) = call.block().and_then(|b| b.as_block_node()) else { return };
        let Some(body) = block.body() else { return };
        let Some(stmts) = body.as_statements_node() else { return };
        for stmt in &stmts.body() {
            self.harvest_extended_eval_stmt(i, &stmt);
        }
    }

    /// One statement of a literal `base.class_eval do ... end` body, where
    /// `self` is the base.
    fn harvest_extended_eval_stmt(&mut self, i: usize, stmt: &Node<'_>) {
        if let Some(def) = stmt.as_def_node() {
            if def.receiver().is_none() {
                let name = String::from_utf8_lossy(def.name().as_slice()).into_owned();
                self.fragments[i].hook_instance_installs.push((name, span_of(stmt)));
            }
            return;
        }
        let Some(call) = stmt.as_call_node() else { return };
        match String::from_utf8_lossy(call.name().as_slice()).as_ref() {
            "send" | "public_send" | "__send__" => {
                self.fragments[i].hook_installs_opaque = true;
            }
            "define_method" => match hook_define_method_name(&call) {
                Some(n) => self.fragments[i].hook_instance_installs.push((n, span_of(stmt))),
                None => self.fragments[i].hook_installs_opaque = true,
            },
            _ => {}
        }
    }

    /// Walk every arm of a `begin/rescue/else/ensure` (bead ita-o8l.5):
    /// the primary body, every `rescue` clause in the chain (`subsequent()`
    /// links a second `rescue Foo` onto the first), the `else` clause (runs
    /// only when nothing raised), and `ensure` (always runs). Shared by
    /// `walk_stmts`' own `as_begin_node` case (a class/module body directly
    /// wrapped in `begin ... end`) and `walk_stmt`'s `Node::BeginNode` arm
    /// (a `begin` used as one class-body statement among several) — same
    /// "conservative: walk every arm" discipline `if/else` uses just below,
    /// since which arm actually executes is unknowable statically and an
    /// indexed name can only turn an existing `NotFound` diagnostic off,
    /// never fabricate a new one (invariant #1).
    fn walk_begin_arms(
        &mut self,
        scope: &str,
        nesting: &[String],
        frag_idx: Option<usize>,
        in_singleton: bool,
        begin: &ruby_prism::BeginNode,
    ) {
        self.walk_begin_arm(scope, nesting, frag_idx, in_singleton, begin.statements());
        let mut rescue = begin.rescue_clause();
        while let Some(r) = rescue {
            self.walk_begin_arm(scope, nesting, frag_idx, in_singleton, r.statements());
            rescue = r.subsequent();
        }
        if let Some(e) = begin.else_clause() {
            self.walk_begin_arm(scope, nesting, frag_idx, in_singleton, e.statements());
        }
        if let Some(ens) = begin.ensure_clause() {
            self.walk_begin_arm(scope, nesting, frag_idx, in_singleton, ens.statements());
        }
    }

    /// One arm of a `begin`. Extracted to keep `walk_begin_arms` under the
    /// complexity ceiling: the four arms differ only in which accessor
    /// yields the statements, so the shared step is worth a name — the
    /// ceiling made the split the cheaper option, which is the point of it.
    fn walk_begin_arm(
        &mut self,
        scope: &str,
        nesting: &[String],
        frag_idx: Option<usize>,
        in_singleton: bool,
        stmts: Option<ruby_prism::StatementsNode<'_>>,
    ) {
        if let Some(stmts) = stmts {
            self.walk_stmts(scope, nesting, frag_idx, in_singleton, &stmts.as_node());
        }
    }

    fn walk_stmts(
        &mut self,
        scope: &str,
        nesting: &[String],
        frag_idx: Option<usize>,
        in_singleton: bool,
        node: &Node<'_>,
    ) {
        if let Some(stmts) = node.as_statements_node() {
            for stmt in &stmts.body() {
                // Bead ita-uh1: a `sig { ... }` immediately followed by a
                // `def` threads its captured `.returns(...)` text into
                // that `def` via `pending_sorbet_ret` (set in the
                // `Node::CallNode` arm below, consumed in `method_def`).
                // Any statement that is neither the `sig` call itself nor
                // the `def` breaks the adjacency — clear it so an
                // unrelated later `def` never inherits a stale sig.
                if !is_sig_call_or_def(&stmt) {
                    self.pending_sorbet_ret = None;
                }
                self.walk_stmt(scope, nesting, frag_idx, in_singleton, &stmt);
            }
        } else if let Some(begin) = node.as_begin_node() {
            self.walk_begin_arms(scope, nesting, frag_idx, in_singleton, &begin);
        } else if let Some(prog) = node.as_program_node() {
            self.walk_stmts(scope, nesting, frag_idx, in_singleton, &prog.statements().as_node());
        } else {
            self.walk_stmt(scope, nesting, frag_idx, in_singleton, node);
        }
    }

    fn walk_stmt(
        &mut self,
        scope: &str,
        nesting: &[String],
        frag_idx: Option<usize>,
        in_singleton: bool,
        node: &Node<'_>,
    ) {
        match node {
            Node::ClassNode { .. } => {
                let class = node.as_class_node().unwrap();
                if let Some(path) = const_path_str(&class.constant_path()) {
                    let full = join_path(scope, &path);
                    // Bead ita-519: exactly ONE new `Module.nesting` entry
                    // per `class`/`module` keyword, whether written
                    // compact (`class A::B::C`) or not — never one entry
                    // per `::`-segment in `full`. See `ProjectIndex::
                    // resolve_const`'s doc comment for why that
                    // distinction matters.
                    let mut child_nesting = nesting.to_vec();
                    child_nesting.push(full.clone());
                    let superclass = class.superclass().and_then(|s| const_path_str(&s));
                    if let Some(body) = class.body() {
                        self.walk_body(&full, &child_nesting, false, &body);
                    } else {
                        self.fragments
                            .push(ClassFragment::new(full.clone(), false, child_nesting));
                    }
                    // superclass lives on the fragment(s) just created for
                    // `full`; attach to the last one with that path. A
                    // dynamic superclass expression (`< Struct.new(...)`)
                    // makes the ancestry unknowable: open.
                    if let Some(f) = self.fragments.iter_mut().rev().find(|f| f.path == full) {
                        match superclass {
                            Some(sc) => f.superclass = Some(sc),
                            None if class.superclass().is_some() => {
                                f.open = true;
                                if f.open_reason.is_none() {
                                    f.open_reason = Some(OpenReason::DynamicSuperclass);
                                }
                            }
                            None => {}
                        }
                        // Bead ita-h6l (mechanism B): reopening a
                        // core/stdlib/gem class — never modeled ancestry,
                        // must stay open regardless of who wrote this
                        // block. First-reason-wins: a dynamic superclass
                        // above already explains the open, if present.
                        if is_known_external_class_path(&full) {
                            f.open = true;
                            if f.open_reason.is_none() {
                                f.open_reason = Some(OpenReason::ReopenedExternal);
                            }
                        }
                    }
                }
            }
            Node::ModuleNode { .. } => {
                let m = node.as_module_node().unwrap();
                if let Some(path) = const_path_str(&m.constant_path()) {
                    let full = join_path(scope, &path);
                    let mut child_nesting = nesting.to_vec();
                    child_nesting.push(full.clone());
                    if let Some(body) = m.body() {
                        self.walk_body(&full, &child_nesting, false, &body);
                    } else {
                        self.fragments
                            .push(ClassFragment::new(full.clone(), true, child_nesting));
                    }
                    if let Some(f) = self.fragments.iter_mut().rev().find(|f| f.path == full) {
                        f.is_module = true;
                    }
                }
            }
            Node::SingletonClassNode { .. } => {
                let sc = node.as_singleton_class_node().unwrap();
                // `class << self` does NOT push a `Module.nesting` entry
                // in real Ruby, so `nesting` passes through unchanged.
                if sc.expression().as_self_node().is_some() {
                    if let Some(body) = sc.body() {
                        self.walk_stmts(scope, nesting, frag_idx, true, &body);
                    }
                } else {
                    // `class << X` where X is a literal constant path:
                    // the body defines methods on X's CLASS OBJECT, and
                    // the file saying so is usually not the file that
                    // opens X. Walking it with `in_singleton` puts every
                    // `def`/`attr_*` on X's singleton track and lets the
                    // existing openness machinery open X for anything
                    // else in that body (singleton-track step N+1).
                    if let Some(owner) = const_path_str(&sc.expression()) {
                        if let Some(body) = sc.body() {
                            let oi = self.fragment_idx_for(&owner, nesting);
                            let child = self.fragments[oi].nesting.clone();
                            self.walk_stmts(&owner, &child, Some(oi), true, &body);
                        }
                    }
                    // The ENCLOSING class stays open regardless, exactly
                    // as before: additive only. A class whose body
                    // reopens someone else's singleton was open here
                    // since bead ita-o1n, and narrowing that in the same
                    // step as modeling X would mix a silence change into
                    // a knowledge change.
                    if let Some(i) = frag_idx {
                        self.open_class(i, OpenReason::SingletonClassExpr);
                    }
                }
            }
            Node::DefNode { .. } => {
                let def = node.as_def_node().unwrap();
                let mut md = self.method_def(&def);
                let treated_as_singleton =
                    in_singleton || def.receiver().is_some_and(|r| r.as_self_node().is_some());
                let name = md.name.clone();
                if let Some(i) = frag_idx {
                    if name == "method_missing" || name == "respond_to_missing?" {
                        self.open_class(i, OpenReason::MethodMissing);
                    }
                    // Singleton-track step N+1, shape (2): a def BODY
                    // that defines methods dynamically. Def bodies are
                    // otherwise never walked, so a class whose class
                    // methods are installed by
                    // `define_singleton_method(key)` inside
                    // `def self.setup` looked CLOSED with none of them
                    // in it — silent today only because the singleton
                    // `NotFound` arm is characterized silent, and a
                    // guaranteed invariant #1 violation the moment it
                    // flips. Measured: discourse's `GlobalSetting`
                    // (app/models/global_setting.rb:5, :69, :262) is
                    // exactly this, 275 residue sites.
                    //
                    // TEXT PREFILTER FIRST, then the AST. Walking every
                    // def body unconditionally cost `check/project_index`
                    // 10.71 ms against a 7.04 ms ceiling (measured; the
                    // perf gate caught it). The overwhelming majority of
                    // method bodies mention none of these names, and a
                    // substring scan over the def's own span is orders
                    // cheaper than a prism walk — the same technique the
                    // `NotImplementedError` scan just below already uses.
                    // The prefilter only ever SKIPS work: a body that
                    // mentions a name still goes through the AST, which
                    // is what decides anything.
                    // Body-ONLY span (bead ita-nst fix): the def's whole
                    // span starts with "def " by construction, so a
                    // prefilter over it is tautological — every body
                    // would pass, the perf ceiling the original prefilter
                    // bought (10.71 ms vs 7.04, measured) would be gone,
                    // and MUT-F's control (the `_exec` prefilter entry)
                    // would read BLIND. The body node starts after the
                    // header/params, which is exactly where a NESTED def
                    // can first appear.
                    let body_text = def.body().map_or("", |b| {
                        let loc = b.location();
                        &self.text[loc.start_offset()..md.def_span.1]
                    });
                    // The `def ` arms are bead ita-nst: a nested def
                    // KEYWORD spells "def " in the BODY text, and
                    // "define_method" does not (the 'd','e','f' are
                    // followed by 'i'). False positives (a comment or
                    // string saying "def ") only cost an AST walk.
                    if BODY_DEF_NAMES.iter().any(|n| body_text.contains(n))
                        || body_text.contains("def ")
                        || body_text.contains("def\n")
                        || body_text.contains("def\t")
                    {
                        let defs = dynamic_defs_in_body(
                            &def,
                            self.text,
                            self.line_index,
                            self.sig_comments,
                        );
                        for (target, reason) in defs.opens {
                            match target {
                                DefTarget::Enclosing => self.open_class(i, reason),
                                DefTarget::Named(path) => {
                                    let oi = self.fragment_idx_for(&path, nesting);
                                    self.open_class(oi, reason);
                                }
                            }
                        }
                        for (target, lit) in defs.literals {
                            self.apply_body_def(
                                i,
                                nesting,
                                treated_as_singleton,
                                md.def_span,
                                target,
                                lit,
                            );
                        }
                        // Bead ita-nst: file every nested `def` KEYWORD.
                        // The self-binding rules (MRI-verified, and the
                        // fixtures carry the proofs):
                        // * enclosing def is singleton-shaped (`def self.x`
                        //   or `class << self`): self at block run time IS
                        //   the class/module, so BOTH spellings define on
                        //   its singleton track — `def y` on the class of
                        //   self (= this module's singleton class) and
                        //   `def self.y` directly on it.
                        // * enclosing INSTANCE def with a plain-yield
                        //   block: `def y` defines on the class of self —
                        //   this very class — so the instance track; but
                        //   `def self.y` defines on ONE object's own
                        //   singleton, an owner no index position can
                        //   name: open (invariant #1 — an optimistic
                        //   filing here could silence a real typo by
                        //   inventing a name the class never gets).
                        // Measured shape: discourse's
                        // `EmotionDashboardReport.fetch_data`, defined in
                        // a plain-yield block inside `def self.register!`
                        // this filing.
                        for nd in defs.nested_defs {
                            let nested_name = nd.md.name.clone();
                            if nested_name == "method_missing" || nested_name == "respond_to_missing?" {
                                self.open_class(i, OpenReason::MethodMissing);
                            }
                            if treated_as_singleton {
                                self.fragments[i].singleton_methods.push(nd.md);
                            } else if nd.receiver_self {
                                self.open_class(i, OpenReason::NestedDefOwner);
                            } else {
                                self.fragments[i].methods.push(nd.md);
                            }
                        }
                    }
                    // ponytail: text scan, not AST — `raise NotImplementedError`
                    // is Ruby's abstract-class idiom; such classes call
                    // subclass hooks we can't see. Proper abstract-method
                    // modeling is v1. The same scan marks the def itself
                    // an abstract stub: its signature is not the one that
                    // runs (`MethodDef::abstract_stub`).
                    let abstract_stub =
                        self.text[md.def_span.0..md.def_span.1].contains("NotImplementedError");
                    if abstract_stub {
                        self.open_class(i, OpenReason::AbstractRaise);
                    }
                    md.abstract_stub = abstract_stub;
                    if treated_as_singleton {
                        self.fragments[i].singleton_methods.push(md);
                        // Bead ita-1yw: `def self.included(base)` may add
                        // instance methods to every includer — harvest the
                        // literal shape (helper is a no-op otherwise).
                        if name == "included" {
                            self.harvest_included_hook(i, &def);
                        }
                        // Bead H of onda 2: `def self.extended(base)` is
                        // the same hook for the `extend` direction — see
                        // `harvest_extended_hook`.
                        if name == "extended" {
                            self.harvest_extended_hook(i, &def);
                        }
                    } else {
                        self.fragments[i].methods.push(md);
                    }
                } else if name == "method_missing" || name == "respond_to_missing?" {
                    // A top-level `def method_missing` lands on Object:
                    // EVERY receiver — core included — dispatches through
                    // it (bead ita-2ve condition (b)). No fragment is
                    // created for toplevel defs, so flag it globally.
                    self.core_mixin = true;
                }
                // def with a non-self receiver (`def obj.foo`) is ignored.
            }
            Node::ConstantWriteNode { .. } => {
                let cw = node.as_constant_write_node().unwrap();
                let name = String::from_utf8_lossy(cw.name().as_slice()).into_owned();
                match frag_idx {
                    Some(i) => self.fragments[i].consts.push(name.clone()),
                    // Defect B (bead ita-exc): a true toplevel write —
                    // `frag_idx == None` — used to land ONLY in the
                    // resolution-inert map below; `ProjectIndex::toplevel_consts`
                    // is what makes it visible to `const_exists`.
                    None => self.toplevel_consts.push(name.clone()),
                }
                // w12 closure: qualified + span for E0104 did-you-mean,
                // recorded even at toplevel (frag_idx None) — `Bar = 1` at
                // toplevel is as suggestible as `module Foo; Bar = 1; end`.
                let loc = cw.name_loc();
                self.consts.push((
                    join_path(scope, &name),
                    loc.start_offset(),
                    loc.end_offset(),
                ));
                // Bead ita-54k: RHS itself a literal constant path is an
                // alias (`X = Y`) — followed by `const_exists` only, via
                // `ProjectIndex::resolve_const_via_alias`. `resolve_const`
                // itself never consults this (invariant #1: widening it
                // could manufacture a type where none was proven).
                if let Some(target) = const_path_str(&cw.value()) {
                    self.const_aliases.push((
                        join_path(scope, &name),
                        nesting.to_vec(),
                        target.trim_start_matches("::").to_string(),
                    ));
                }
            }
            Node::ConstantPathWriteNode { .. } => {
                // Qualified value-constant assignment (`A::B::C = ...` —
                // Tapioca's standard form for gem constants, W3). The
                // target IS a constant path; harvest the fully-qualified
                // name into `consts` (suppression/suggestion only, never
                // a type — same contract as the simple-write arm above).
                if let Some(cpw) = node.as_constant_path_write_node() {
                    if let Some(path) = const_path_str(&cpw.target().as_node()) {
                        let full = path.trim_start_matches("::").to_string();
                        let (s, e) = span_of(node);
                        self.consts.push((full.clone(), s, e));
                        // Defect B (bead ita-exc): also index this write
                        // for resolution, not just did-you-mean — see
                        // `resolve_qualified_const_writes`.
                        match full.rsplit_once("::") {
                            Some((owner, simple)) => {
                                self.qualified_writes.push((owner.to_string(), simple.to_string()));
                            }
                            // Bead ita-9he: no `::` left after trimming the
                            // leading cbase marker means the target WAS the
                            // cbase form of a single segment (`::X = v`,
                            // `cpw.target()`'s `parent()` is `None` — see
                            // `const_path_str`'s cbase branch). Ruby defines
                            // this on `Object` exactly like a bare toplevel
                            // `X = v` — route it into the SAME channel the
                            // `ConstantWriteNode` arm's `frag_idx == None`
                            // case feeds, regardless of `frag_idx`/lexical
                            // nesting here: cbase ignores enclosing
                            // class/module bodies entirely, so a `::X = v`
                            // written inside a class still defines top-level
                            // `X`, not a member of that class.
                            None => self.toplevel_consts.push(full.clone()),
                        }
                        // Bead ita-54k: same alias contract as the simple
                        // write arm above, for the qualified LHS shape
                        // (`A::B = C::D`).
                        if let Some(target) = const_path_str(&cpw.value()) {
                            self.const_aliases.push((
                                full,
                                nesting.to_vec(),
                                target.trim_start_matches("::").to_string(),
                            ));
                        }
                    }
                }
            }
            Node::CallNode { .. } => {
                let call = node.as_call_node().unwrap();
                // bead ita-2ve: a method-injection call whose receiver is
                // a constant naming a core class/mixin (`String.prepend(M)`,
                // `Kernel.class_eval { def ... }`, `Hash.send(:define_method,
                // ...)`) adds methods no fragment records, so the
                // closed-world core lookup must stand down for the run.
                if call
                    .receiver()
                    .is_some_and(|r| core_injection_call(&r, call.name().as_slice()))
                {
                    self.core_mixin = true;
                }
                // A block running at class-body level (`FIELDS.each do
                // define_method ... end`, `included do ... end`) can define
                // anything: open, whatever the receiver — except
                // `define_method` itself with a literal name (symbol or
                // string): that's the one block-taking call whose defined
                // method we actually know, so it indexes instead of
                // blinding the whole class (bead ita-53y); and, bead
                // ita-4xy, a RECOGNIZED `sig { ... }` block (see
                // `sig_block_is_recognized`'s doc comment): pure Sorbet
                // type metadata that never itself defines a method, same
                // reasoning as the `define_method` carve-out. Any other
                // shape (dynamic `define_method` name, explicit receiver,
                // an UNRECOGNIZED `sig` block, or a block on any other
                // call) still opens it — narrowing further would risk a
                // false E0101 the way bead ita-d0j's dynamic-include gap
                // did.
                // Bead ita-sce (2026-09-21, found by the inference bench's
                // own `singleton_class_eval` row the day the class-object
                // track armed): `X.singleton_class.class_eval do
                // define_method(:generate) { ... } end` installs a real
                // CLASS METHOD on `X` through a body this walker does not
                // read. `class << X` and `X.singleton_class.prepend M`
                // were already held-aside singleton patches; the
                // eval/send family on the same receiver was not, so `X`
                // read as a complete, closed class object and
                // `X.generate` became a false E0101 the moment the arm
                // started emitting. Fail-closed: the patch carries no
                // methods, only openness, and `declared_owner_required`
                // makes `merge_file_fragments` drop it when the project
                // never declares `X` (the TCPSocket rule — inventing the
                // class is the defect this avoids).
                if let Some(owner) = singleton_class_owner(&call) {
                    if is_eval_name(call.name().as_slice())
                        || matches!(
                            call.name().as_slice(),
                            b"send"
                                | b"public_send"
                                | b"__send__"
                                | b"define_method"
                                | b"define_singleton_method"
                                | b"alias_method"
                                | b"attr_reader"
                                | b"attr_writer"
                                | b"attr_accessor"
                        )
                    {
                        let mut frag =
                            ClassFragment::new(owner, false, nesting.to_vec());
                        frag.declared_owner_required = true;
                        frag.open = true;
                        frag.open_reason = Some(OpenReason::EvalOrSend);
                        self.fragments.push(frag);
                        return;
                    }
                }
                if call.block().is_some_and(|b| b.as_block_node().is_some()) {
                    let Some(i) = frag_idx else { return };
                    if call.name().as_slice() == b"define_method" && call.receiver().is_none() {
                        let name = call
                            .arguments()
                            .and_then(|a| a.arguments().iter().next())
                            .and_then(|a| literal_method_name(&a));
                        match name {
                            Some(m) => {
                                let mut md = MethodDef::synthetic(m, 0, span_of(node));
                                md.arity_unknown = true;
                                // `define_method(:x) { ... }` — the
                                // BLOCK spelling, and the common one.
                                // Same track routing as the argument
                                // form below.
                                self.track(i, in_singleton).push(md);
                            }
                            None => self.open_class(i, OpenReason::DynamicDefineMethod),
                        }
                    } else if call.name().as_slice() == b"sig" && call.receiver().is_none() {
                        // Sorbet `sig { ... }` (bead ita-uh1): captured
                        // purely as data for the immediately-following
                        // `def` (threaded through `pending_sorbet_ret` by
                        // `walk_stmts`). Bead ita-4xy narrows the open
                        // marking: only an UNRECOGNIZED shape still opens
                        // — `extract_sig_return`/`sig_block_is_recognized`
                        // share one shape check, so "recognized" here is
                        // exactly "the text `method_return`'s fallback
                        // could ever consume, or a `void` sig with none to
                        // consume", never a broader guess. A `sig` call
                        // this bead can't classify (multi-statement block,
                        // unrecognized outermost call, ...) is exactly as
                        // unknown as before — still opens.
                        self.pending_sorbet_ret = extract_sig_return(self.text, &call);
                        if !sig_block_is_recognized(&call) {
                            self.open_class(i, OpenReason::ClassBodyBlock);
                        }
                    } else if call.name().as_slice() == b"class_methods"
                        && call.receiver().is_none()
                        && !scope.is_empty()
                        && is_concern_edge(&self.fragments[i].extends)
                    {
                        // `class_methods do ... end` (singleton-track
                        // family (c)): ActiveSupport::Concern's block
                        // form of the `ClassMethods` module — the gem
                        // literally defines `ClassMethods` from this
                        // block. Harvested into a synthetic fragment for
                        // that exact path plus the same `extends` edge
                        // `apply_concern_class_methods` adds for the
                        // written-out form, so both spellings resolve
                        // through one mechanism.
                        //
                        // GATED ON THE CONCERN EDGE
                        // (`is_concern_edge_before`): a receiverless
                        // `class_methods` call is only this DSL when the
                        // module extends ActiveSupport::Concern — without
                        // the edge the harvest would INVENT
                        // `M::ClassMethods` (fragment + edge + methods)
                        // for a module whose block may never execute as
                        // this DSL at all, and an invented CLOSED surface
                        // is exactly what the flip cannot be allowed to
                        // accuse on. Measured 2026-09-18 across the
                        // public corpora: 123 of 125 `class_methods do`
                        // sites have `extend ActiveSupport::Concern`
                        // lexically BEFORE the block in the same file
                        // (rails 19/21, discourse 83/83, mastodon
                        // 21/21); the misses fall through to the `_`
                        // catch-all below — the module opens exactly as
                        // it did, and the harvest loss is a documented
                        // false negative, never a diagnostic (invariant
                        // #1).
                        //
                        // OPENNESS IS PRESERVED: the block still opens
                        // the concern, exactly as before. Closing it
                        // would unmask everything else the index cannot
                        // see inside a concern body (mattr_accessor's
                        // measured 28-false-positive lesson), so this
                        // banks the names and changes nothing observable
                        // today.
                        self.harvest_class_methods_block(i, scope, nesting, &call);
                        self.open_class(i, OpenReason::ClassBodyBlock);
                    } else {
                        if let Some(block) = call.block() {
                            self.harvest_block_consts(i, &block);
                            self.harvest_block_nested_definitions(scope, nesting, &block);
                        }
                        self.harvest_interpolated_eval_defs(i, &call);
                        self.open_class(i, OpenReason::ClassBodyBlock);
                    }
                    return;
                }
                // `self.table_name = "literal"` / `self.table_name = expr`:
                // the model->table override this bead's contract gives
                // priority over convention. A dynamic RHS is captured too
                // (as `Dynamic`) so the convention fallback doesn't kick in
                // for a class that clearly overrides it some other way —
                // silence over a wrong guess.
                if call.name().as_slice() == b"table_name="
                    && call.receiver().is_some_and(|r| r.as_self_node().is_some())
                {
                    if let Some(i) = frag_idx {
                        let arg = call.arguments().and_then(|a| a.arguments().iter().next());
                        self.fragments[i].table_name =
                            Some(match arg.as_ref().and_then(Node::as_string_node) {
                                Some(s) => TableNameDecl::Literal(
                                    String::from_utf8_lossy(s.unescaped()).into_owned(),
                                ),
                                None => TableNameDecl::Dynamic,
                            });
                    }
                    return;
                }
                // `include`/`extend`/`prepend` are checked before the
                // generic receiver bailout below: a receiver that isn't
                // implicit self (`T.unsafe(self).include Foo`) means we
                // don't know which object is actually being mixed into, so
                // the class is open regardless of the argument shape —
                // otherwise dynamic mixins reached through a wrapper call
                // silently kept the class closed and produced a false
                // E0101 (bead ita-d0j). Implicit or explicit `self`
                // receiver keeps resolving exactly as before: a literal
                // constant path argument still closes ancestry normally,
                // any other argument shape still opens it.
                if matches!(call.name().as_slice(), b"include" | b"extend" | b"prepend") {
                    // `X.singleton_class.include/prepend M` — the mixin
                    // lands on X's CLASS OBJECT, so it is exactly the
                    // `extend` edge `lookup_singleton` already walks.
                    // Runs before the fragment-less bailout below on
                    // purpose: the shape's whole point is patching a
                    // class from a file that never opens it.
                    //
                    // `.singleton_class.extend M` is a different animal
                    // (M lands on the singleton's OWN singleton, i.e.
                    // X's class methods' class methods): nothing here
                    // models that, so it opens X rather than guessing.
                    if let Some(owner) = singleton_class_owner(&call) {
                        let oi = self.fragment_idx_for(&owner, nesting);
                        if call.name().as_slice() == b"extend" {
                            self.open_class(oi, OpenReason::SingletonClassExpr);
                            return;
                        }
                        let mut all_literal = true;
                        if let Some(args) = call.arguments() {
                            for arg in &args.arguments() {
                                match const_path_str(&arg) {
                                    Some(path) => {
                                        if !self.fragments[oi].extends.contains(&path) {
                                            self.fragments[oi].extends.push(path);
                                        }
                                    }
                                    None => all_literal = false,
                                }
                            }
                        }
                        if !all_literal {
                            self.open_class(oi, OpenReason::DynamicMixinArg);
                        }
                        return;
                    }
                    // Top-level (fragment-less) bare `include M` mixes
                    // into Object, so every core receiver can gain M's
                    // methods (bead ita-2ve condition (b)). Bare
                    // top-level `extend`/`prepend` only touch `main`'s
                    // singleton — not a core-class patch.
                    if frag_idx.is_none() {
                        if call.receiver().is_none() && call.name().as_slice() == b"include" {
                            self.core_mixin = true;
                        }
                        return;
                    }
                    let i = frag_idx.expect("non-None: guarded above");
                    let name = String::from_utf8_lossy(call.name().as_slice()).into_owned();
                    let implicit_or_self =
                        call.receiver().is_none_or(|r| r.as_self_node().is_some());
                    if !implicit_or_self {
                        self.open_class(i, OpenReason::DynamicMixinReceiver);
                        return;
                    }
                    if let Some(args) = call.arguments() {
                        for arg in &args.arguments() {
                            if let Some(path) = const_path_str(&arg) {
                                match name.as_str() {
                                    // Bead ita-scl: inside `class << self`
                                    // an `include`/`prepend` lands on the
                                    // SINGLETON class's ancestry, which is
                                    // the chain a class-object call
                                    // dispatches through — the same place
                                    // an `extend` on the class body puts a
                                    // module. Filing it as an instance
                                    // include both missed the singleton
                                    // surface (mastodon's
                                    // `class << self; include Redisable`
                                    // in `app/lib/delivery_failure_tracker.rb:50`,
                                    // leaving the bare `redis` at :71 a
                                    // conclusive miss) and claimed an
                                    // instance surface the code never gets.
                                    "include" | "prepend" if in_singleton => {
                                        if !self.fragments[i].extends.contains(&path) {
                                            self.fragments[i].extends.push(path);
                                        }
                                    }
                                    "include" => self.fragments[i].includes.push(path),
                                    "extend" => self.fragments[i].extends.push(path),
                                    _ => self.fragments[i].prepends.push(path),
                                }
                            } else if arg.as_self_node().is_some()
                                && name == "extend"
                                && !scope.is_empty()
                            {
                                // `extend self` (singleton-track family
                                // (b)): the module's own instance methods
                                // become its class-object methods. That is
                                // exactly what an `extend <own path>` edge
                                // already means to `lookup_singleton`,
                                // which reads an extended module's
                                // `methods` map onto the extender's
                                // singleton — so the shape needs no new
                                // mechanism, only the edge. It also stops
                                // opening the class for a mixin argument
                                // it now understands; `extend self` is
                                // the one `extend` argument whose target
                                // is never in doubt.
                                self.fragments[i].extends.push(scope.to_string());
                            } else {
                                // Dynamic mixin: can't know the ancestry.
                                self.open_class(i, OpenReason::DynamicMixinArg);
                            }
                        }
                    }
                    return;
                }
                // W3 require/autoload: literal `require '<lib>'` with an
                // implicit receiver, at any nesting level — Ruby's
                // `require` is process-global, so a lib required anywhere
                // in the project defines its constants everywhere. This
                // runs BEFORE the `frag_idx` bailout so toplevel requires
                // (the overwhelmingly common shape) are captured too.
                // Only a StringNode argument counts: `require var` can
                // name anything, and never feeding a non-literal into the
                // stdlib gate keeps it fact-based (invariant #1).
                if call.name().as_slice() == b"require" && call.receiver().is_none() {
                    if let Some(lib) = call
                        .arguments()
                        .and_then(|a| a.arguments().iter().next())
                        .and_then(|a| a.as_string_node())
                    {
                        let lib = String::from_utf8_lossy(lib.unescaped()).into_owned();
                        if !lib.is_empty() && !self.requires.contains(&lib) {
                            self.requires.push(lib);
                        }
                    }
                    return;
                }
                // `singleton_class.attr_reader/attr_writer/attr_accessor
                // :a, :b` — the RECEIVER spelling of `class << self`
                // `attr_*` (singleton-track family (a), second spelling).
                // The class-body call arm below only ever saw RECEIVERLESS
                // calls (the early return just under swallows everything
                // else), so this shape fell through every arm: nothing was
                // filed, nothing opened. Measured 2026-09-18 at de28b40 —
                // the residue probe's largest rails family:
                // `ActiveSupport::Dependencies.interlock/autoload_paths/...`
                // (`dependencies.rb:10,48`, ~45 explicit-receiver sites),
                // `ActionDispatch::ExceptionWrapper.rescue_responses/...`
                // (`exception_wrapper.rb:12`, ~27), `ActiveModel::
                // Translation.raise_on_missing_translations`
                // (`translation.rb:25`), activesupport's own
                // `ClassAttributeTest::Prepending.read/write` — every one
                // a `singleton_class.attr_accessor :name` whose names no
                // lookup could find on a class the index believed CLOSED.
                //
                // `is_own_singleton_class` is the receiver test: a bare
                // `singleton_class` (or `self.singleton_class`) means the
                // ENCLOSING class object — the same target family (a)'s
                // `class << self` spelling files on. A constant receiver
                // (`X.singleton_class.attr_accessor`) is NOT taken here:
                // from inside `class Y` that call usually still means Y's
                // own class object at runtime, but taking it by NAME would
                // need the declared-owner machinery
                // (`apply_singleton_patches`), and no corpus site uses the
                // shape — narrow until measured (invariant #1).
                //
                // OPENNESS IS PRESERVED at its pre-arm state: CLOSED. This
                // spelling never opened the class before (the early return
                // swallowed it silently), and filing names is strictly
                // additive — `NotFound` becomes `Found`, never the
                // reverse. Unlike the receiverless macros below (mattr_*,
                // class_attribute), whose catch-all openness must be
                // preserved OPEN because closing unmasks what the index
                // cannot see (the measured 28-false-positive lesson), this
                // shape has no catch-all history to preserve: the methods
                // land on a closed class, and the re-measured residue plus
                // the public gate are the instruments that police any
                // name the class carries that this filing does not see.
                // A non-symbol argument defines an unknowable method set:
                // that alone OPENS (DynamicAttrArg), the same rule as the
                // `class << self` spelling above.
                if call.receiver().is_some_and(|r| is_own_singleton_class(&r)) {
                    if matches!(
                        call.name().as_slice(),
                        b"attr_reader" | b"attr_writer" | b"attr_accessor"
                    ) {
                        if let Some(i) = frag_idx {
                            let mut all_symbols = true;
                            if let Some(args) = call.arguments() {
                                for arg in &args.arguments() {
                                    if let Some(sym) = arg.as_symbol_node() {
                                        let attr = String::from_utf8_lossy(sym.unescaped())
                                            .into_owned();
                                        let aspan = span_of(&arg);
                                        let mut defs: Vec<MethodDef> = Vec::new();
                                        if call.name().as_slice() != b"attr_writer" {
                                            defs.push(MethodDef::synthetic(
                                                attr.clone(),
                                                0,
                                                aspan,
                                            ));
                                        }
                                        if call.name().as_slice() != b"attr_reader" {
                                            defs.push(MethodDef::synthetic(
                                                format!("{attr}="),
                                                1,
                                                aspan,
                                            ));
                                        }
                                        self.fragments[i].singleton_methods.extend(defs);
                                    } else {
                                        all_symbols = false;
                                    }
                                }
                            }
                            if !all_symbols {
                                self.open_class(i, OpenReason::DynamicAttrArg);
                            }
                        }
                    }
                    return;
                }
                if call.receiver().is_some() {
                    return;
                }
                let Some(i) = frag_idx else { return };
                let name = String::from_utf8_lossy(call.name().as_slice()).into_owned();
                let span = span_of(node);
                match name.as_str() {
                    // `attr_reader`/`attr_writer`/`attr_accessor`. Inside
                    // `class << self` the macro defines the methods on the
                    // CLASS OBJECT, not on instances (bead singleton-track
                    // family (a), measured 2026-09-17: the residue probe
                    // found `RequireProfiler.stats`-shaped sites whose
                    // reader the index had filed on the instance track, so
                    // the singleton lookup walked a closed class and saw
                    // nothing). Routing by `in_singleton` is additive on
                    // both tracks at once: a name moves from a track where
                    // nothing ever looked it up to the track that does, and
                    // no lookup that used to be `Found` can stop being
                    // found — an instance-track `attr_*` inside
                    // `class << self` was never a real instance method in
                    // MRI either (the fixture proves it: MRI raises on
                    // `Config.new.stats`).
                    "attr_reader" | "attr_writer" | "attr_accessor" => {
                        let mut all_symbols = true;
                        if let Some(args) = call.arguments() {
                            for arg in &args.arguments() {
                                if let Some(sym) = arg.as_symbol_node() {
                                    let attr =
                                        String::from_utf8_lossy(sym.unescaped()).into_owned();
                                    let aspan = span_of(&arg);
                                    let mut defs: Vec<MethodDef> = Vec::new();
                                    if name != "attr_writer" {
                                        defs.push(MethodDef::synthetic(attr.clone(), 0, aspan));
                                    }
                                    if name != "attr_reader" {
                                        defs.push(MethodDef::synthetic(
                                            format!("{attr}="),
                                            1,
                                            aspan,
                                        ));
                                    }
                                    let frag = &mut self.fragments[i];
                                    let track = if in_singleton {
                                        &mut frag.singleton_methods
                                    } else {
                                        &mut frag.methods
                                    };
                                    track.extend(defs);
                                } else {
                                    all_symbols = false;
                                }
                            }
                        }
                        if !all_symbols {
                            self.open_class(i, OpenReason::DynamicAttrArg);
                        }
                    }
                    // ActiveSupport's `mattr_accessor` family (`cattr_*` is
                    // the same macro under its older name). It defines BOTH
                    // tracks: the class-object accessor and, unless told
                    // otherwise, the instance accessor. Until now the index
                    // modeled neither — `mattr_accessor` appeared zero
                    // times in this file — which is why
                    // `ActiveRecord::Migrator.migrations_paths=` and
                    // `ActiveSupport::JSON::Encoding.json_encoder` looked
                    // absent from a class whose ancestry the index
                    // considered closed.
                    //
                    // Additive to the index only: it can make a lookup
                    // Found that used to be NotFound/Inconclusive, never
                    // the reverse, so it is monotonically LESS diagnostic
                    // (invariant #1). Options are honored because getting
                    // them wrong invents a method that does not exist:
                    // `instance_accessor: false` suppresses both instance
                    // sides, `instance_reader:`/`instance_writer: false`
                    // one each. Anything not understood (a dynamic name, a
                    // non-symbol option key) opens the class instead of
                    // guessing, exactly as the `attr_*` arm above does.
                    // `thread_mattr_*`/`thread_cattr_*`
                    // (`attribute_accessors_per_thread.rb`) is the same
                    // macro family with a thread-local backing store: the
                    // singleton reader/writer are ALWAYS defined, the
                    // instance reader needs `instance_reader` AND
                    // `instance_accessor`, the instance writer needs
                    // `instance_writer` AND `instance_accessor` — the same
                    // option shape this arm already parses, so they join
                    // the same arm rather than forking a second one.
                    // Measured 2026-09-18: zero residue sites carry these
                    // names today, but any real ActiveSupport project can
                    // call them, and the task of the track is that the
                    // index never meets a class-body macro it does not
                    // know (the mattr comment above is the precedent).
                    "mattr_accessor" | "mattr_reader" | "mattr_writer" | "cattr_accessor"
                    | "cattr_reader" | "cattr_writer" | "thread_mattr_accessor"
                    | "thread_mattr_reader" | "thread_mattr_writer" | "thread_cattr_accessor"
                    | "thread_cattr_reader" | "thread_cattr_writer" => {
                        let wants_reader = !name.ends_with("_writer");
                        let wants_writer = !name.ends_with("_reader");
                        let mut instance_reader = wants_reader;
                        let mut instance_writer = wants_writer;
                        let mut attrs: Vec<(String, (usize, usize))> = Vec::new();
                        let mut all_known = true;
                        if let Some(args) = call.arguments() {
                            for arg in &args.arguments() {
                                if let Some(sym) = arg.as_symbol_node() {
                                    attrs.push((
                                        String::from_utf8_lossy(sym.unescaped()).into_owned(),
                                        span_of(&arg),
                                    ));
                                } else if let Some(kw) = arg.as_keyword_hash_node() {
                                    for el in &kw.elements() {
                                        let key = el
                                            .as_assoc_node()
                                            .map(|a| (a.key(), a.value()));
                                        let Some((k, v)) = key else {
                                            all_known = false;
                                            continue;
                                        };
                                        let Some(ks) = k.as_symbol_node() else {
                                            all_known = false;
                                            continue;
                                        };
                                        let is_false = v.as_false_node().is_some();
                                        match String::from_utf8_lossy(ks.unescaped()).as_ref() {
                                            "instance_accessor" if is_false => {
                                                instance_reader = false;
                                                instance_writer = false;
                                            }
                                            "instance_reader" if is_false => {
                                                instance_reader = false;
                                            }
                                            "instance_writer" if is_false => {
                                                instance_writer = false;
                                            }
                                            // `default:` and friends create
                                            // no method and need no opening.
                                            _ => {}
                                        }
                                    }
                                } else {
                                    all_known = false;
                                }
                            }
                        }
                        for (attr, aspan) in attrs {
                            if wants_reader {
                                self.fragments[i].singleton_methods.push(
                                    MethodDef::synthetic(attr.clone(), 0, aspan),
                                );
                            }
                            if wants_writer {
                                self.fragments[i].singleton_methods.push(
                                    MethodDef::synthetic(format!("{attr}="), 1, aspan),
                                );
                            }
                            if instance_reader {
                                self.fragments[i].methods.push(MethodDef::synthetic(
                                    attr.clone(),
                                    0,
                                    aspan,
                                ));
                            }
                            if instance_writer {
                                self.fragments[i].methods.push(MethodDef::synthetic(
                                    format!("{attr}="),
                                    1,
                                    aspan,
                                ));
                            }
                        }
                        // OPENNESS IS PRESERVED, deliberately. Before this
                        // arm existed the macro fell through to the `_`
                        // catch-all and opened the class
                        // (`UnknownClassBodyCall`). Handling it here would
                        // otherwise CLOSE classes that used to be open, and
                        // closing a class does not add knowledge — it only
                        // unmasks whatever the index already could not see.
                        // Measured on discourse 2026-09-17: `TopicQuery`'s
                        // only class-body opener is
                        // `cattr_accessor :results_filter_callbacks`, and
                        // closing it produced 28 new E0101 on methods that
                        // are real — installed by the plugin API
                        // (`add_to_class(:topic_query, :list_group_topics_assigned)`)
                        // from another file, which no index here can model.
                        // So this arm records the accessors and leaves the
                        // class exactly as open as it was: strictly
                        // additive knowledge, zero diagnostic drift
                        // (invariant #1). The resolution gain is gated on
                        // class openness, which is a separate problem —
                        // see the mattr_accessor tests.
                        self.open_class(i, OpenReason::UnknownClassBodyCall);
                        if !all_known {
                            self.open_class(i, OpenReason::DynamicAttrArg);
                        }
                    }
                    // ActiveSupport's `class_attribute :a, :b` (read out of
                    // the gem's own source at the version rails locks,
                    // `core_ext/class/attribute.rb`): the SINGLETON track
                    // always gets the reader and the writer, plus the
                    // `a?` predicate unless `instance_predicate: false` —
                    // the predicate class_eval defines `self.a?` (class
                    // object) and, when `instance_reader` is on, `a?` on
                    // instances. The instance reader/writer follow the
                    // documented option chain: `instance_reader` and
                    // `instance_writer` each DEFAULT to `instance_accessor`,
                    // so `instance_accessor: false` suppresses both unless
                    // the finer option re-enables it explicitly. A dynamic
                    // option value (`instance_reader: flag`) is read
                    // conservatively as "defines everything": inventing the
                    // absence of a method that may exist is the one
                    // direction this file never guesses in (invariant #1).
                    //
                    // OPENNESS IS PRESERVED, the same rule as the mattr arm
                    // above: receiverless today, the macro falls through to
                    // the `_` catch-all and opens the class
                    // (`UnknownClassBodyCall`), and this arm keeps it
                    // exactly that open. The filing's resolution gain is
                    // therefore gated on class openness — banked knowledge,
                    // observable once openness itself is solved, never a
                    // diagnostic change (invariant #1).
                    "class_attribute" => {
                        let mut instance_accessor = Some(true);
                        let mut instance_reader: Option<bool> = None;
                        let mut instance_writer: Option<bool> = None;
                        let mut instance_predicate = Some(true);
                        let mut attrs: Vec<(String, (usize, usize))> = Vec::new();
                        let mut all_known = true;
                        if let Some(args) = call.arguments() {
                            for arg in &args.arguments() {
                                if let Some(sym) = arg.as_symbol_node() {
                                    attrs.push((
                                        String::from_utf8_lossy(sym.unescaped()).into_owned(),
                                        span_of(&arg),
                                    ));
                                } else if let Some(kw) = arg.as_keyword_hash_node() {
                                    for el in &kw.elements() {
                                        let key = el
                                            .as_assoc_node()
                                            .map(|a| (a.key(), a.value()));
                                        let Some((k, v)) = key else {
                                            all_known = false;
                                            continue;
                                        };
                                        let Some(ks) = k.as_symbol_node() else {
                                            all_known = false;
                                            continue;
                                        };
                                        // Only a LITERAL `false` suppresses.
                                        // A dynamic value may define the
                                        // method, so it reads as `true`.
                                        let literal_false = v.as_false_node().is_some();
                                        let literal_true = v.as_true_node().is_some();
                                        let value = if literal_false {
                                            Some(false)
                                        } else if literal_true {
                                            Some(true)
                                        } else {
                                            None
                                        };
                                        match String::from_utf8_lossy(ks.unescaped()).as_ref() {
                                            "instance_accessor" => instance_accessor = value,
                                            "instance_reader" => instance_reader = value,
                                            "instance_writer" => instance_writer = value,
                                            "instance_predicate" => instance_predicate = value,
                                            // `default:` and friends create
                                            // no method and need no opening.
                                            _ => {}
                                        }
                                    }
                                } else {
                                    all_known = false;
                                }
                            }
                        }
                        // The documented default chain: each fine-grained
                        // option falls back to `instance_accessor`, and a
                        // DYNAMIC anything falls forward to "defined"
                        // (conservative).
                        let eff_accessor = instance_accessor.unwrap_or(true);
                        let eff_reader = instance_reader.unwrap_or(eff_accessor);
                        let eff_writer = instance_writer.unwrap_or(eff_accessor);
                        let eff_predicate = instance_predicate
                            .unwrap_or(true);
                        for (attr, aspan) in attrs {
                            self.fragments[i].singleton_methods.push(
                                MethodDef::synthetic(attr.clone(), 0, aspan),
                            );
                            self.fragments[i]
                                .singleton_methods
                                .push(MethodDef::synthetic(format!("{attr}="), 1, aspan));
                            if eff_predicate {
                                self.fragments[i]
                                    .singleton_methods
                                    .push(MethodDef::synthetic(format!("{attr}?"), 0, aspan));
                            }
                            if eff_reader {
                                self.fragments[i].methods.push(MethodDef::synthetic(
                                    attr.clone(),
                                    0,
                                    aspan,
                                ));
                            }
                            if eff_writer {
                                self.fragments[i]
                                    .methods
                                    .push(MethodDef::synthetic(format!("{attr}="), 1, aspan));
                            }
                            if eff_predicate && eff_reader {
                                self.fragments[i]
                                    .methods
                                    .push(MethodDef::synthetic(format!("{attr}?"), 0, aspan));
                            }
                        }
                        self.open_class(i, OpenReason::UnknownClassBodyCall);
                        if !all_known {
                            self.open_class(i, OpenReason::DynamicAttrArg);
                        }
                    }
                    // Routed by `in_singleton` exactly like `attr_*`
                    // above (commit eeb4e6a): in a `class << self` body
                    // these define CLASS-OBJECT methods, and filing them
                    // on the instance track put them where no singleton
                    // lookup ever looks while inventing an instance
                    // method MRI does not have.
                    "define_method" => {
                        let sym = call
                            .arguments()
                            .and_then(|a| a.arguments().iter().next())
                            .and_then(|a| literal_method_name(&a));
                        match sym {
                            Some(m) => {
                                let mut md = MethodDef::synthetic(m, 0, span);
                                md.arity_unknown = true;
                                self.track(i, in_singleton).push(md);
                            }
                            None => self.open_class(i, OpenReason::DynamicDefineMethod),
                        }
                    }
                    "alias_method" => {
                        let mut args = call
                            .arguments()
                            .map(|a| a.arguments().iter().collect::<Vec<_>>())
                            .unwrap_or_default()
                            .into_iter();
                        let new = args.next().and_then(|a| {
                            a.as_symbol_node()
                                .map(|s| String::from_utf8_lossy(s.unescaped()).into_owned())
                        });
                        match new {
                            Some(m) => {
                                let mut md = MethodDef::synthetic(m, 0, span);
                                md.arity_unknown = true;
                                self.track(i, in_singleton).push(md);
                            }
                            None => self.open_class(i, OpenReason::DynamicAliasMethod),
                        }
                    }
                    "class_eval"
                    | "module_eval"
                    | "instance_eval"
                    | "send"
                    | "public_send"
                    | "__send__"
                    | "delegate"
                    | "define_singleton_method" => {
                        self.open_class(i, OpenReason::EvalOrSend);
                    }
                    // Sorbet `T::Helpers#mixes_in_class_methods ::X` — the
                    // `ActiveSupport::Concern` `ClassMethods` idiom
                    // (`included do extend X end`), rendered literally by
                    // Tapioca into every RBI that reopens a Concern. Real
                    // runtime behavior: `X`'s instance methods land on the
                    // includer's SINGLETON, same as an actual `extend`.
                    // Project-side lookup still treats this exactly like
                    // any other unmodeled class-body call (opens the
                    // fragment, unconditionally, below) — modeling the
                    // real effect for project code is out of scope here.
                    // The argument is captured purely so the RBI-only
                    // method closure (bead ita-xze) can walk it as a
                    // singleton-track edge.
                    "mixes_in_class_methods" => {
                        if let Some(args) = call.arguments() {
                            for arg in &args.arguments() {
                                if let Some(path) = const_path_str(&arg) {
                                    self.fragments[i].mixes_in_class_methods.push(path);
                                }
                            }
                        }
                        self.open_class(i, OpenReason::UnknownClassBodyCall);
                    }
                    // Visibility modifiers: harmless, but `private def foo`
                    // wraps the def as an argument — index it.
                    //
                    // `module_function` is not harmless (singleton-track
                    // family (b)): it ALSO copies the module's instance
                    // methods onto the module object, which is how
                    // `ActionCable.server` (`module_function def server`)
                    // and `Mastodon::Version.user_agent` (bare
                    // `module_function`, 49 and 2 residue sites measured
                    // 2026-09-17) are real methods the index could not
                    // see. Modeled as the same `extend <own path>` edge
                    // `extend self` uses above, which is a deliberate
                    // OVER-approximation in one direction only: the real
                    // macro affects the defs that follow it (bare form) or
                    // its arguments, and this edge exposes every instance
                    // method of the module on its singleton. Wrong only
                    // ever by resolving a name whose call site is already
                    // a `NoMethodError` at runtime — never by hiding one,
                    // and never by fabricating a diagnostic on working
                    // code (invariant #1).
                    "private"
                    | "public"
                    | "protected"
                    | "module_function"
                    | "private_class_method"
                    | "public_class_method" => {
                        if name == "module_function" && !scope.is_empty() {
                            self.fragments[i].extends.push(scope.to_string());
                        }
                        if let Some(args) = call.arguments() {
                            for arg in &args.arguments() {
                                if arg.as_def_node().is_some() {
                                    self.walk_stmt(scope, nesting, frag_idx, in_singleton, &arg);
                                }
                            }
                        }
                    }
                    // Known no-ops for method definition purposes.
                    "require" | "require_relative" | "private_constant" | "public_constant"
                    | "freeze" | "puts" | "raise" => {}
                    // Bead ita-o1n, the same idea one step further. The
                    // blocker census found classes whose ONLY reason for
                    // being open is a framework call that registers a hook
                    // or a validator and defines nothing — 23.9% of
                    // corpus-a's `ancestry open`, 18.2% of corpus-b's,
                    // and by `inconclusive_reason`'s precedence rule all of
                    // it on chains with NO external blocker, so closing
                    // them is a real conclusiveness gain needing zero gem
                    // knowledge. See `defines_no_method` for why the
                    // exclusions are the safety argument.
                    n if defines_no_method(n) => {}
                    // Any other bare call in a class body is a DSL or
                    // metaprogramming (`has_many`, Dry's `option`,
                    // `each do ... define_method ... end`): it may define
                    // methods we cannot see. Invariant #1: open the class.
                    _ => {
                        self.open_class(i, OpenReason::UnknownClassBodyCall);
                    }
                }
            }
            // `alias foo bar` keyword form — same track routing as
            // `alias_method` above.
            Node::AliasMethodNode { .. } => {
                let al = node.as_alias_method_node().unwrap();
                if let Some(i) = frag_idx {
                    if let Some(sym) = al.new_name().as_symbol_node() {
                        let mut md = MethodDef::synthetic(
                            String::from_utf8_lossy(sym.unescaped()).into_owned(),
                            0,
                            span_of(node),
                        );
                        md.arity_unknown = true;
                        self.track(i, in_singleton).push(md);
                    }
                }
            }
            // if/unless guards around defs etc. — conservative: walk both arms.
            Node::IfNode { .. } => {
                let n = node.as_if_node().unwrap();
                if let Some(s) = n.statements() {
                    self.walk_stmts(scope, nesting, frag_idx, in_singleton, &s.as_node());
                }
                if let Some(s) = n.subsequent() {
                    self.walk_stmts(scope, nesting, frag_idx, in_singleton, &s);
                }
            }
            Node::UnlessNode { .. } => {
                let n = node.as_unless_node().unwrap();
                if let Some(s) = n.statements() {
                    self.walk_stmts(scope, nesting, frag_idx, in_singleton, &s.as_node());
                }
            }
            Node::ElseNode { .. } => {
                let n = node.as_else_node().unwrap();
                if let Some(s) = n.statements() {
                    self.walk_stmts(scope, nesting, frag_idx, in_singleton, &s.as_node());
                }
            }
            // `begin ... rescue ... else ... ensure ... end` used as a
            // class-body statement (bead ita-o8l.5), e.g. the corpus site
            // `activesupport/.../instrumenter.rb`'s two alternative
            // `def now_cpu` — one in the `begin` arm, one in `rescue`.
            // Same "conservative: walk every arm" discipline as `if/else`
            // right above: which arm actually runs is unknowable
            // statically, so a `def` in ANY of them is indexed.
            // Suppression-only under invariant #1 — an indexed name can
            // only turn an existing NotFound diagnostic off, never
            // fabricate a new one. Shared with `walk_stmts`' own
            // `as_begin_node` case (a class/module body directly wrapped
            // in `begin/rescue`) via `walk_begin_arms`, so both shapes
            // get the same rescue/else/ensure coverage from one place.
            // The sibling arms above reach for `.unwrap()` after matching,
            // but those predate the policy ratchet and are frozen by it;
            // new code takes the `if let` chain instead. Matching the shape
            // and then unwrapping the accessor is dispatch done twice, and
            // the second one is the one that can panic.
            Node::BeginNode { .. } => {
                if let Some(n) = node.as_begin_node() {
                    self.walk_begin_arms(scope, nesting, frag_idx, in_singleton, &n);
                }
            }
            _ => {}
        }
    }

    fn method_def(&mut self, def: &ruby_prism::DefNode<'_>) -> MethodDef {
        build_method_def(
            def,
            self.text,
            self.line_index,
            self.sig_comments,
            &mut self.sig_errors,
            self.pending_sorbet_ret.take(),
        )
    }
}

/// `DefWalker::method_def` as a free function (bead ita-nst): the nested-
/// def collector inside `dynamic_defs_in_body`'s scanner needs the exact
/// same construction but holds no `DefWalker` — only the three references
/// this function needs. The ONE deliberate difference: nested defs take
/// no `pending_sorbet_ret` (`sig`/`def` adjacency across a block boundary
/// is not a thing Tapioca or a human writes) and their `#:` sig-comment
/// lookup still runs, same as any def.
fn build_method_def(
    def: &ruby_prism::DefNode<'_>,
    text: &str,
    line_index: &LineIndex,
    sig_comments: &HashMap<u32, (usize, usize)>,
    sig_errors: &mut Vec<(usize, usize, String)>,
    pending_sorbet_ret: Option<String>,
) -> MethodDef {
    let name = String::from_utf8_lossy(def.name().as_slice()).into_owned();
    let name_loc = def.name_loc();
    let def_loc = def.location();
        let mut md = MethodDef {
            name,
            required: 0,
            optional: 0,
            rest: false,
            keywords: Vec::new(),
            kwrest: false,
            block: false,
            sig: None,
            sorbet_ret: pending_sorbet_ret,
            arity_unknown: false,
            abstract_stub: false,
            name_span: (name_loc.start_offset(), name_loc.end_offset()),
            def_span: (def_loc.start_offset(), def_loc.end_offset()),
        };

        if let Some(params) = def.parameters() {
            fill_def_params(&mut md, &params);
        }

        // `#:` sig on the line right above the def.
        let (def_line, _) = line_index.line_col(text, def_loc.start_offset());
        if def_line > 0 {
            if let Some(&(cstart, cend)) = sig_comments.get(&(def_line - 1)) {
                let body = text[cstart..cend].trim_start_matches("#:").trim();
                match parse_rbs_comment(body) {
                    Ok(sig) => md.sig = Some(sig),
                    Err(e) => sig_errors.push((cstart, cend, e)),
                }
            }
        }
        md
    }

/// The parameters block of `build_method_def` (bead ita-nst split for the
/// complexity ceiling): counts, rest/forwarding, keywords, block.
fn fill_def_params(md: &mut MethodDef, params: &ruby_prism::ParametersNode<'_>) {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a single `def`'s parameter count is bounded by the source file it was parsed from, far below u32::MAX"
    )]
    {
        md.required = (params.requireds().iter().count() + params.posts().iter().count()) as u32;
        md.optional = params.optionals().iter().count() as u32;
    }
    md.rest = params.rest().is_some()
        // `def foo(...)` (Ruby 3.0 forwarding, ita-7g9): prism
        // parses `...` as the params' `keyword_rest` field holding
        // a `ForwardingParameterNode`, not `rest`. Forwarding
        // relays whatever positional/keyword/block args the
        // caller passes, so it models the same "no cap" arity as
        // a bare `*args` — treat it as rest for `check_arity`
        // (crates/itaruby_semantic/src/check.rs), which already
        // skips both the min and max check when `rest` is true.
        || params
            .keyword_rest()
            .is_some_and(|kr| kr.as_forwarding_parameter_node().is_some());
    for kw in &params.keywords() {
        if let Some(k) = kw.as_required_keyword_parameter_node() {
            md.keywords.push((
                String::from_utf8_lossy(k.name().as_slice()).into_owned(),
                true,
            ));
        } else if let Some(k) = kw.as_optional_keyword_parameter_node() {
            md.keywords.push((
                String::from_utf8_lossy(k.name().as_slice()).into_owned(),
                false,
            ));
        }
    }
    md.kwrest = params.keyword_rest().is_some();
    md.block = params.block().is_some();
}

fn span_of(node: &Node<'_>) -> (usize, usize) {
    let loc = node.location();
    (loc.start_offset(), loc.end_offset())
}

/// True for a `def`, or for a bare `sig { ... }` call — the two shapes
/// `walk_stmts`' adjacency check (bead ita-uh1) never clears
/// `pending_sorbet_ret` for. Deliberately loose about whether the `sig`
/// call's block is actually a recognized return-type shape: even an
/// unrecognized `sig` still legitimately precedes the `def` it types
/// (with `sorbet_ret` staying `None`), so it must not be treated as an
/// unrelated statement that breaks adjacency.
fn is_sig_call_or_def(node: &Node<'_>) -> bool {
    node.as_def_node().is_some()
        || node.as_call_node().is_some_and(|c| {
            c.name().as_slice() == b"sig" && c.receiver().is_none() && c.block().is_some()
        })
}

/// Shared block-unwrapping for a `sig { ... }` call: the block's single
/// statement, or `None` for a call with no block, a block whose body
/// isn't exactly one statement, or an empty block. Factored out so
/// `extract_sig_return` (the `.returns(X)` text extractor) and
/// `sig_block_is_recognized` (bead ita-4xy's open-class carve-out) share
/// one shape check instead of two.
fn sig_block_stmt<'pr>(call: &ruby_prism::CallNode<'pr>) -> Option<Node<'pr>> {
    let block = call.block()?.as_block_node()?;
    let body = block.body()?;
    match body.as_statements_node() {
        Some(stmts) => {
            let mut iter = stmts.body().iter();
            let first = iter.next()?;
            if iter.next().is_some() {
                return None;
            }
            Some(first)
        }
        None => Some(body),
    }
}

/// Every call name `body_def_reason` can react to. Used ONLY as a cheap
/// substring prefilter over a `def`'s own source span before the AST
/// walk (`dynamic_defs_in_body`): a body naming none of these cannot
/// produce a finding, and skipping the walk for it is what keeps
/// `check/project_index` inside its ceiling (measured: walking every
/// body unconditionally cost 10.71 ms against a 7.04 ms ceiling). Keep
/// in sync with `body_def_reason` — a name here it ignores only costs a
/// wasted walk; a name MISSING here is a missed finding.
/// Proven by measurement, not by reading: the first version of this list
/// omitted `instance_eval`, and the residue probe came back with 10 MORE
/// discourse sites than the unfiltered walk — the exact "a name missing
/// here is a missed finding" failure the sentence above describes.
/// `dynamic_def_prefilter_covers_every_reacting_name` now pins the list
/// against `body_def_reason`.
const BODY_DEF_NAMES: [&str; 13] = [
    "eval",
    "define_method",
    "define_singleton_method",
    "alias_method",
    "attr_reader",
    "attr_writer",
    "attr_accessor",
    "class_eval",
    "module_eval",
    "instance_eval",
    "instance_exec",
    "class_exec",
    "module_exec",
];

/// Does this `def`'s BODY define methods in a way the index cannot
/// enumerate — and if so, with which reason?
///
/// Def bodies are never walked by `walk_stmt` (methods are recorded,
/// their contents are the checker's business, not the index's), which
/// left a hole the singleton track cannot afford: discourse's
/// `GlobalSetting` installs every one of its class methods with
/// `define_singleton_method(key)` inside `def self.load_defaults` and
/// friends (`app/models/global_setting.rb:5`, `:69`, `:262`), and the
/// class looked CLOSED with none of those names in it. That is silent
/// today only because the `Ty::Class` `NotFound` arm is characterized
/// silent; it is a guaranteed invariant #1 violation the moment the arm
/// reports, and 275 residue sites on that one class.
///
/// ATTRIBUTION BY RECEIVER, because the first version opened the
/// enclosing class for any of these calls anywhere in the body and that
/// is measurably too coarse: rails' `route_set.rb:526` writes
/// `MountedHelpers.class_eval do ... end` inside
/// `def self.mounted_helpers`, which defines methods on
/// `MountedHelpers` and says nothing at all about `RouteSet`. So:
///
/// * implicit or `self` receiver -> the ENCLOSING class (`None`): in a
///   `def self.x` body `self` IS the class, so `define_method` there
///   really does define its instance methods and
///   `define_singleton_method` its class methods
/// * literal constant receiver -> that constant, by name (`Some(path)`,
///   applied through `apply_singleton_patches`' declared-owner rule)
/// * dynamic receiver (`mod.define_method`, `route_set.rb:340`) ->
///   NOTHING: an unknown object's methods are not evidence about this
///   class, and opening it anyway would blanket most of any real app
///
/// DEFINITION shapes only. A dynamic DISPATCH (`send`, `public_send`)
/// proves nothing about what a class defines, so it is deliberately not
/// here.
///
/// A LITERAL `define_method(:name)`/`define_singleton_method(:name)`
/// opens nothing: the name is knowable, and pretending otherwise trades
/// a fact for a blanket. Those names are REGISTERED instead — see
/// `BodyDefs::literals` and `ClassWalk::apply_body_def`, where the
/// receiver decides the fragment and the enclosing `def`'s own kind
/// decides whether `self` is provably the class.
fn dynamic_defs_in_body(
    def: &ruby_prism::DefNode<'_>,
    text: &str,
    line_index: &LineIndex,
    sig_comments: &HashMap<u32, (usize, usize)>,
) -> BodyDefs {
    struct Scan<'pr> {
        found: BodyDefs,
        text: &'pr str,
        line_index: &'pr LineIndex,
        sig_comments: &'pr HashMap<u32, (usize, usize)>,
    }
    impl Scan<'_> {
        fn note(&mut self, target: DefTarget, reason: OpenReason) {
            if !self.found.opens.iter().any(|(t, _)| *t == target) {
                self.found.opens.push((target, reason));
            }
        }

        /// Record what this one call says, and answer whether the walk
        /// should stop here. Inside `X.class_eval { ... }` the block's
        /// `self` is X, so an implicit-receiver `define_method(name)` in
        /// there defines on X — NOT on the class lexically around the
        /// `def`. Measured on rails: attributing that nested call to the
        /// enclosing class opened `ActionDispatch::Routing::RouteSet`
        /// and took a baseline E0101 with it.
        ///
        /// A call either makes its target unknowable (a reason) or names
        /// exactly what it defines (literals) — never both, which is
        /// what keeps a half-literal `attr_accessor :a, b` from filing
        /// `a` while its own blanket is the honest answer.
        fn consume(&mut self, node: &ruby_prism::CallNode<'_>) -> bool {
            let Some(target) = body_def_target(node) else { return false };
            let names_its_target = matches!(target, DefTarget::Named(_));
            match body_def_reason(node) {
                Some(reason) => self.note(target, reason),
                None => {
                    for lit in body_def_literals(node) {
                        self.found.literals.push((target.clone(), lit));
                    }
                }
            }
            names_its_target && is_eval_name(node.name().as_slice())
        }
    }
    impl<'pr> ruby_prism::Visit<'pr> for Scan<'pr> {
        fn visit_call_node(&mut self, node: &ruby_prism::CallNode<'pr>) {
            if self.consume(node) {
                return;
            }
            ruby_prism::visit_call_node(self, node);
        }

        /// Bead ita-nst: a `def` KEYWORD nested anywhere in the body
        /// (usually inside a plain-yield block — discourse's
        /// `EmotionDashboardReport.register!` shape). The walk starts at
        /// the ENCLOSING def's body, so every `DefNode` seen here is a
        /// nested one. The `Visit` trait's reference lifetime cannot be
        /// stored, so the OWNED `MethodDef` is built here and the arm's
        /// filing rules only read `receiver_self` and the def itself's
        /// data. Recursion continues — defs inside defs run under the
        /// same self-binding rules.
        fn visit_def_node(&mut self, node: &ruby_prism::DefNode<'pr>) {
            let md = build_method_def(
                node,
                self.text,
                self.line_index,
                self.sig_comments,
                &mut Vec::new(),
                None,
            );
            self.found.nested_defs.push(NestedDef {
                md,
                receiver_self: node.receiver().is_some_and(|r| r.as_self_node().is_some()),
            });
            ruby_prism::visit_def_node(self, node);
        }
    }
    let Some(body) = def.body() else { return BodyDefs::default() };
    let mut scan = Scan {
        found: BodyDefs::default(),
        text,
        line_index,
        sig_comments,
    };
    ruby_prism::Visit::visit(&mut scan, &body);
    scan.found
}

/// Everything a `def` body says about some class's surface: the targets
/// whose surface it makes unknowable, and the exact methods it defines
/// where the call names them.
#[derive(Default)]
struct BodyDefs {
    opens: Vec<(DefTarget, OpenReason)>,
    literals: Vec<(DefTarget, BodyDefLiteral)>,
    /// Bead ita-nst: every `def` KEYWORD nested in this body, in walk
    /// order — the walker files them per the self-binding rules in
    /// `walk_stmt`'s `Node::DefNode` arm.
    nested_defs: Vec<NestedDef>,
}

/// One nested `def` keyword found by the body scanner (bead ita-nst).
struct NestedDef {
    md: MethodDef,
    /// `def self.x` spelling — under an instance-method enclosure this is
    /// the unattributable shape (`OpenReason::NestedDefOwner`).
    receiver_self: bool,
}

/// One method a literal definer inside a `def` body installs.
struct BodyDefLiteral {
    /// `define_singleton_method` writes the CLASS-OBJECT track; every
    /// other definer here writes the instance track.
    singleton: bool,
    name: String,
    required: u32,
    /// `define_method`/`alias_method` take their arity from a block or
    /// from another method, so the index knows the name and nothing
    /// else — arity checking must stand down (invariant #1).
    arity_unknown: bool,
}

/// The methods a method-defining call inside a `def` body installs, when
/// it names all of them. Empty for every call `body_def_reason` reacts
/// to (those are blankets, not facts) and for every name this walker
/// does not model.
fn body_def_literals(node: &ruby_prism::CallNode<'_>) -> Vec<BodyDefLiteral> {
    let args: Vec<Node<'_>> =
        node.arguments().map(|a| a.arguments().iter().collect()).unwrap_or_default();
    let name = node.name();
    body_def_literals_named(name.as_slice(), &args)
}

fn body_def_literals_named(name: &[u8], args: &[Node<'_>]) -> Vec<BodyDefLiteral> {
    let opaque_arity = |m: String, singleton: bool| {
        vec![BodyDefLiteral { singleton, name: m, required: 0, arity_unknown: true }]
    };
    let first = || args.first().and_then(literal_method_name);
    match name {
        b"define_method" | b"alias_method" => {
            first().map(|m| opaque_arity(m, false)).unwrap_or_default()
        }
        b"define_singleton_method" => first().map(|m| opaque_arity(m, true)).unwrap_or_default(),
        b"attr_reader" | b"attr_writer" | b"attr_accessor" => body_def_attr_literals(name, args),
        // The same one-level unwrap `body_def_reason_named` does, so
        // `send(:attr_accessor, :mode)` files the same two methods
        // the bare call would.
        b"send" | b"public_send" | b"__send__" => match args.first().and_then(literal_method_name)
        {
            Some(inner) => body_def_literals_named(inner.as_bytes(), &args[1..]),
            None => Vec::new(),
        },
        _ => Vec::new(),
    }
}

/// `attr_accessor :a` in a `def self.x` body: reader and writer with
/// their REAL arities, which is the whole observable gain — a
/// wrong-arity call on one of these is an `ArgumentError` MRI really
/// raises (`def_body_attr_accessor_arity_accuses.rb`).
fn body_def_attr_literals(name: &[u8], args: &[Node<'_>]) -> Vec<BodyDefLiteral> {
    let mut out = Vec::new();
    for arg in args {
        let Some(m) = literal_method_name(arg) else { return Vec::new() };
        if name != b"attr_writer" {
            out.push(BodyDefLiteral {
                singleton: false,
                name: m.clone(),
                required: 0,
                arity_unknown: false,
            });
        }
        if name != b"attr_reader" {
            out.push(BodyDefLiteral {
                singleton: false,
                name: format!("{m}="),
                required: 1,
                arity_unknown: false,
            });
        }
    }
    out
}

/// Whose surface a method-defining call inside a `def` body describes.
#[derive(Clone, PartialEq, Eq)]
enum DefTarget {
    /// Implicit or `self` receiver: the class the `def` is written in.
    /// In a `def self.x` body `self` IS the class, so `define_method`
    /// there really defines its instance methods and
    /// `define_singleton_method` its class methods.
    Enclosing,
    /// A literal constant receiver — that class, by name, applied
    /// through `apply_singleton_patches`' declared-owner rule.
    Named(String),
}

/// `None` for a dynamic receiver: an unknown object's methods are
/// evidence about nothing, and opening the enclosing class for them
/// would blanket most of any real app.
fn body_def_target(node: &ruby_prism::CallNode<'_>) -> Option<DefTarget> {
    // `X.singleton_class.alias_method(...)`: the class object, reached
    // exactly the way shape (1) reaches it.
    if let Some(owner) = singleton_class_owner(node) {
        return Some(DefTarget::Named(owner));
    }
    match node.receiver() {
        None => Some(DefTarget::Enclosing),
        Some(r) if r.as_self_node().is_some() => Some(DefTarget::Enclosing),
        // Bare `singleton_class.alias_method(...)` in a `def self.x`
        // body: self's own class object. Without this the receiver reads
        // as "some unknown object" and a real, knowable singleton edit
        // is filed as evidence about nothing — caught by
        // `every_dynamic_def_shape_opens_its_class`.
        Some(r) if is_own_singleton_class(&r) => Some(DefTarget::Enclosing),
        Some(r) => const_path_str(&r).map(DefTarget::Named),
    }
}

fn is_own_singleton_class(node: &Node<'_>) -> bool {
    node.as_call_node().is_some_and(|inner| {
        inner.name().as_slice() == b"singleton_class"
            && inner.arguments().is_none()
            && inner.receiver().is_none_or(|b| b.as_self_node().is_some())
    })
}

/// The `_eval`/`_exec` rebind family: inside any of these blocks the
/// block's `self` is the receiver, so a definition there lands on an
/// object no index position can name. One predicate because
/// `body_def_reason` treats them identically (`EvalOrSend`) and
/// `BODY_DEF_NAMES` must list all six or the prefilter drops the body
/// before `body_def_reason` is ever consulted
/// (`dynamic_def_prefilter_covers_every_reacting_name` pins the pair).
fn is_eval_name(name: &[u8]) -> bool {
    matches!(
        name,
        b"class_eval"
            | b"module_eval"
            | b"instance_eval"
            | b"instance_exec"
            | b"class_exec"
            | b"module_exec"
    )
}

/// The reason a method-defining call in a `def` body makes its target's
/// surface unknowable, or `None` when the call names everything it
/// defines. Split out of the visitor above to keep it under the
/// complexity ceiling — `L1.COMPLEXITY_CEILING` caught the inline
/// version at 15 paths against 12, doing exactly its job.
fn body_def_reason(node: &ruby_prism::CallNode<'_>) -> Option<OpenReason> {
    let args: Vec<Node<'_>> =
        node.arguments().map(|a| a.arguments().iter().collect()).unwrap_or_default();
    let name = node.name();
    body_def_reason_named(name.as_slice(), &args)
}

/// `body_def_reason` on an already-unwrapped `(name, args)` pair, so
/// `send(:define_method, name)` reuses every rule exactly — the same
/// split `definer_sources_named` uses for the core-pollution walker.
fn body_def_reason_named(name: &[u8], args: &[Node<'_>]) -> Option<OpenReason> {
    if is_eval_name(name) {
        return Some(OpenReason::EvalOrSend);
    }
    match name {
        b"define_method" | b"define_singleton_method" => args
            .first()
            .is_none_or(|a| literal_method_name(a).is_none())
            .then_some(OpenReason::DynamicDefineMethod),
        b"alias_method" => (!args.iter().all(|a| literal_method_name(a).is_some()))
            .then_some(OpenReason::DynamicAliasMethod),
        b"attr_reader" | b"attr_writer" | b"attr_accessor" => {
            (!args.iter().all(|a| a.as_symbol_node().is_some()))
                .then_some(OpenReason::DynamicAttrArg)
        }
        // `send(:define_method, :x)` is a definition wearing a
        // dispatch: unwrap one level and every rule above applies
        // unchanged. A DYNAMIC first argument (`send(what, ...)`)
        // stays `None` deliberately — reacting to it would mean
        // opening a class for every `self.send` in a method body, a
        // conclusiveness change that needs its own corpus measurement
        // and its own prefilter entry (this literal form needs
        // neither: the inner name is spelled in the body text, which
        // is what `BODY_DEF_NAMES` scans —
        // `the_send_form_passes_the_prefilter` pins that).
        // Bead ita-evb: a bare `eval(<expr>)` inside a method body runs
        // a string this index cannot read, in the enclosing class's own
        // scope — `def self.compile_key_builder` in discourse's
        // `lib/middleware/anonymous_cache.rb:33-43` builds
        // `"def self.__compiled_key_builder(h) ... end"` and `eval`s it,
        // which is the ONLY definition of that method in the tree. The
        // class-body spelling already opened its class (`is_eval_name`
        // above, and the class-body `_ =>` arm); the def-body spelling
        // did not, so `AnonymousCache.__compiled_key_builder`
        // (`anonymous_cache.rb:47`) read as a conclusive miss on code
        // that runs. A zero-argument `eval` names nothing and is left
        // alone, exactly like every other shape here.
        b"eval" => (!args.is_empty()).then_some(OpenReason::EvalOrSend),
        b"send" | b"public_send" | b"__send__" => args
            .first()
            .and_then(literal_method_name)
            .and_then(|inner| body_def_reason_named(inner.as_bytes(), &args[1..])),
        _ => None,
    }
}

/// Raw text of a `sig { ... }` call's `.returns(...)` argument (bead
/// ita-uh1). Recognizes exactly the shapes Tapioca actually emits in
/// gem RBIs: `sig { returns(X) }`, `sig { void }`, `sig {
/// params(...).returns(X) }`, `sig { params(...).void }`, and any of
/// those with a leading `override.`/`abstract.` (`sig(:final)`'s own
/// argument is irrelevant here — only the block body matters). The block
/// body must be exactly one statement whose OUTERMOST call is `returns`
/// with exactly one positional argument; `void`, more than one
/// statement, more than one `returns` argument, or any other shape all
/// return `None` — never a guess, matching every other `None` this
/// walker produces.
fn extract_sig_return(text: &str, call: &ruby_prism::CallNode<'_>) -> Option<String> {
    let stmt = sig_block_stmt(call)?;
    let outer = stmt.as_call_node()?;
    if outer.name().as_slice() != b"returns" {
        return None;
    }
    let args = outer.arguments()?;
    let mut arg_iter = args.arguments().iter();
    let arg = arg_iter.next()?;
    if arg_iter.next().is_some() {
        return None;
    }
    let (s, e) = span_of(&arg);
    Some(text[s..e].trim().to_string())
}

/// Bead ita-4xy: does `call`'s block classify as a RECOGNIZED sig shape
/// — same `sig_block_stmt` shape check `extract_sig_return` uses, outer
/// call named `returns` (exactly one positional argument, exactly like
/// `extract_sig_return` requires before it slices the text) OR bare
/// `void` (zero return-type text to consume, but still a real,
/// classified sig — see `extract_sig_return`'s own doc comment: `void`
/// legitimately returns `None` there without being "unrecognized").
/// `false` for anything `extract_sig_return` would also refuse to guess
/// at: multi-statement block, no block at all, or an outermost call
/// that's neither `returns` nor `void`. The `Node::CallNode` arm above
/// is the only caller — this decides whether the class-body `sig` call
/// opens the class, never anything about the type it maps to.
fn sig_block_is_recognized(call: &ruby_prism::CallNode<'_>) -> bool {
    let Some(outer) = sig_block_stmt(call).and_then(|s| s.as_call_node()) else {
        return false;
    };
    match outer.name().as_slice() {
        b"returns" => outer.arguments().is_some_and(|args| {
            let mut iter = args.arguments().iter();
            iter.next().is_some() && iter.next().is_none()
        }),
        b"void" => true,
        _ => false,
    }
}

fn join_path(scope: &str, name: &str) -> String {
    if scope.is_empty() || name.starts_with("::") {
        name.trim_start_matches("::").to_string()
    } else {
        format!("{scope}::{name}")
    }
}

/// Bead ita-47y: `target` (an alias's chased leaf text, possibly itself
/// RBI-only) followed by every remaining `::`-segment an original
/// reference still carries past the segment that alias replaced. See
/// `ProjectIndex::expand_unresolved_alias_target`'s doc comment.
fn join_remaining<'a>(target: &str, rest: impl Iterator<Item = &'a str>) -> String {
    let rest: Vec<&str> = rest.collect();
    if rest.is_empty() {
        target.to_string()
    } else {
        format!("{target}::{}", rest.join("::"))
    }
}

/// `X.singleton_class` as a RECEIVER: the constant path `X`, when the
/// receiver of this call is exactly a no-argument `singleton_class` send
/// to a literal constant path. Anything else — a dynamic base
/// (`obj.singleton_class`), arguments, a chained
/// `X.singleton_class.singleton_class` — is `None`, because this is the
/// only shape whose target object is not in doubt.
///
/// Measured on discourse 2026-09-17: `spec/support/
/// discourse_event_helper.rb` writes
/// `DiscourseEvent.singleton_class.prepend DiscourseEvent::TestHelper`,
/// which is how 205 explicit-receiver `DiscourseEvent.track_events`
/// sites get a method the class itself never defines. `FileScan`'s
/// `note_dynamic_mixin` DID see that call, but classified `prepend` as
/// `MixinTrack::Instance` — true of a normal `prepend`, wrong through
/// `singleton_class`, where include/prepend land on the CLASS OBJECT.
fn singleton_class_owner(call: &ruby_prism::CallNode<'_>) -> Option<String> {
    let recv = call.receiver()?;
    let inner = recv.as_call_node()?;
    if inner.name().as_slice() != b"singleton_class" || inner.arguments().is_some() {
        return None;
    }
    const_path_str(&inner.receiver()?)
}

/// Literal constant path (`Foo`, `Foo::Bar`, `::Foo`) as a string; None for
/// dynamic expressions.
pub fn const_path_str(node: &Node<'_>) -> Option<String> {
    if let Some(c) = node.as_constant_read_node() {
        return Some(String::from_utf8_lossy(c.name().as_slice()).into_owned());
    }
    if let Some(c) = node.as_constant_path_node() {
        let name = c.name()?;
        let name = String::from_utf8_lossy(name.as_slice()).into_owned();
        return match c.parent() {
            Some(parent) => {
                let prefix = const_path_str(&parent)?;
                Some(format!("{prefix}::{name}"))
            }
            None => Some(format!("::{name}")), // `::Foo` — cbase
        };
    }
    None
}

/// The method-lookup track a mixin call feeds, or `None` for a call that
/// is not a mixin at all.
fn mixin_track(name: &[u8]) -> Option<MixinTrack> {
    match name {
        b"include" | b"prepend" => Some(MixinTrack::Instance),
        b"extend" => Some(MixinTrack::Singleton),
        _ => None,
    }
}

/// A `cond ? A : B` (or the keyword `if`/`else` spelling, which prism
/// encodes identically) whose TWO arms are both constant paths — the
/// Rails `defined?(::AppBuilder) ? ::AppBuilder : Rails::AppBuilder`
/// shape. `None` for every other conditional: an `elsif` chain is an
/// `IfNode` nested in `subsequent`, not an `ElseNode`, and any arm that
/// is not a constant path (a call, `Y.new`, a literal) names nothing
/// this scan may attribute a mixin edge to.
///
/// The predicate is deliberately NOT consulted. Whatever it evaluates
/// to, the VALUE of the whole expression is one of exactly the two arms,
/// so a mixin called on that value can only ever target one of those
/// two classes — that is the whole argument, and it needs no knowledge
/// of `defined?` at all.
fn ternary_const_paths(node: &Node<'_>) -> Option<Vec<String>> {
    let if_node = node.as_if_node()?;
    let then_arm = if_node.statements()?.as_node();
    let then_path = const_path_str(&sole_statement(&then_arm)?)?;
    let else_node = if_node.subsequent()?.as_else_node()?;
    let else_arm = else_node.statements()?.as_node();
    let else_path = const_path_str(&sole_statement(&else_arm)?)?;
    Some(vec![then_path, else_path])
}

/// The single expression of a one-statement block/arm, or `None` when the
/// arm is empty or holds more than one statement (an arm whose value
/// would then be its LAST statement — knowable, but not this scan's
/// shape; fail closed).
fn sole_statement<'pr>(node: &Node<'pr>) -> Option<Node<'pr>> {
    let stmts = node.as_statements_node()?;
    let mut body = stmts.body().iter();
    let only = body.next()?;
    body.next().is_none().then_some(only)
}

/// The name of a block that takes exactly one required positional
/// parameter (`do |method|`), or `None` for every other block shape —
/// optionals, rest, keywords, destructuring and a missing parameter all
/// mean the interpolation below is not a straight substitution of one
/// literal list element.
fn sole_block_param(block: &ruby_prism::BlockNode<'_>) -> Option<String> {
    let params = block.parameters()?.as_block_parameters_node()?.parameters()?;
    if !params.optionals().is_empty()
        || params.rest().is_some()
        || !params.posts().is_empty()
        || !params.keywords().is_empty()
        || params.keyword_rest().is_some()
        || params.block().is_some()
    {
        return None;
    }
    let mut required = params.requireds().iter();
    let only = required.next()?;
    if required.next().is_some() {
        return None;
    }
    let name = only.as_required_parameter_node()?;
    Some(String::from_utf8_lossy(name.name().as_slice()).into_owned())
}

/// Every element of a literal array, as its string value — all-or-nothing:
/// one non-literal element (`%w(a) + [x]`, a splat, an interpolation)
/// yields nothing at all, never a partial list.
fn literal_string_elements(array: &ruby_prism::ArrayNode<'_>) -> Vec<String> {
    array
        .elements()
        .iter()
        .map(|element| literal_method_name(&element))
        .collect::<Option<Vec<_>>>()
        .unwrap_or_default()
}

/// One reconstructable eval template: the string as written, with the
/// loop parameter left in place as `#{param}`, plus the span it was
/// written at (the span every name harvested from it is filed with).
struct EvalTemplate {
    text: String,
    span: (usize, usize),
    /// The class the eval call names EXPLICITLY (`Other.class_eval`), as
    /// written — `None` for a receiverless or `self` receiver, which runs
    /// its body in the enclosing class (`eval_target`'s own rule). The
    /// harvest files names only where the body really runs, so a template
    /// naming another class is skipped rather than filed on the wrong one
    /// (see `harvest_interpolated_eval_defs`).
    explicit_target: Option<String>,
}

/// Every `class_eval`/`module_eval`/`instance_eval`/`eval`-family call in
/// `body` whose first argument is an interpolated string literal, rebuilt
/// as text so a literal list element can be substituted into it. A string
/// that interpolates anything other than the loop parameter itself
/// (`#{other}`, `#@ivar`, a method call) is skipped — this is a
/// substitution, not an interpreter.
fn eval_string_templates(body: &Node<'_>, param: &str) -> Vec<EvalTemplate> {
    let mut scan = EvalTemplateScan { param, templates: Vec::new() };
    scan.visit(body);
    scan.templates
}

struct EvalTemplateScan<'a> {
    param: &'a str,
    templates: Vec<EvalTemplate>,
}

impl<'pr> Visit<'pr> for EvalTemplateScan<'_> {
    fn visit_call_node(&mut self, node: &ruby_prism::CallNode<'pr>) {
        if is_eval_name(node.name().as_slice()) {
            let arg = node.arguments().and_then(|a| a.arguments().iter().next());
            if let Some(interp) = arg.as_ref().and_then(Node::as_interpolated_string_node) {
                // The receiver is read exactly as `eval_target` reads it
                // for the pollution mark: a namable constant path is an
                // explicit target, an implicit or `self` receiver is not
                // (its body runs in the enclosing class).
                let explicit_target = node
                    .receiver()
                    .filter(|r| r.as_self_node().is_none())
                    .and_then(|r| const_path_str(&r));
                if let Some(template) = self.template_of(&interp, explicit_target) {
                    self.templates.push(template);
                }
            }
        }
        ruby_prism::visit_call_node(self, node);
    }
}

impl EvalTemplateScan<'_> {
    fn template_of(
        &self,
        node: &ruby_prism::InterpolatedStringNode<'_>,
        explicit_target: Option<String>,
    ) -> Option<EvalTemplate> {
        let mut text = String::new();
        for part in &node.parts() {
            if let Some(s) = part.as_string_node() {
                text.push_str(&String::from_utf8_lossy(s.unescaped()));
            } else {
                let embedded = part.as_embedded_statements_node()?;
                let stmts = embedded.statements()?.as_node();
                let read = sole_statement(&stmts)?.as_local_variable_read_node()?;
                if String::from_utf8_lossy(read.name().as_slice()) != self.param {
                    return None;
                }
                text.push_str("#{");
                text.push_str(self.param);
                text.push('}');
            }
        }
        Some(EvalTemplate { text, span: span_of(&node.as_node()), explicit_target })
    }
}

/// The names of every `def` written directly in `source` — a body
/// substituted out of an eval template, parsed on its own. Zero names for
/// anything prism reports an error on: a substitution that produces
/// invalid Ruby defines nothing this checker may file (fail-closed).
fn top_level_def_names(source: &str) -> Vec<String> {
    let parse = ruby_prism::parse(source.as_bytes());
    if parse.errors().next().is_some() {
        return Vec::new();
    }
    let Some(program) = parse.node().as_program_node() else { return Vec::new() };
    let Some(stmts) = program.statements().as_node().as_statements_node() else { return Vec::new() };
    stmts
        .body()
        .iter()
        .filter_map(|stmt| stmt.as_def_node())
        .map(|def| String::from_utf8_lossy(def.name().as_slice()).into_owned())
        .collect()
}

/// Is `recv.method_name` a method-injection call on a core class/mixin
/// (bead ita-2ve)? `String.prepend(M)`, `Kernel.class_eval { def ... }`,
/// `Hash.send(:define_method, :x)` — each can add instance methods to a
/// core class without creating any fragment, which `by_path` alone can
/// never see. Only constant receivers naming a core namespace count:
/// `Foo.prepend(M)` patches a project class (visible as a fragment), and
/// a dynamic receiver can't be named at all — both stay exactly as v0.
fn core_injection_call(recv: &Node<'_>, name: &[u8]) -> bool {
    const INJECTIONS: &[&str] = &[
        "include",
        "extend",
        "prepend",
        "class_eval",
        "module_eval",
        "instance_eval",
        "class_exec",
        "module_exec",
        "instance_exec",
        "send",
        "public_send",
        "__send__",
        "define_method",
        "define_singleton_method",
        "alias_method",
        "attr_accessor",
        "attr_reader",
        "attr_writer",
    ];
    let Ok(name) = std::str::from_utf8(name) else {
        return false;
    };
    INJECTIONS.contains(&name)
        && const_path_str(recv).is_some_and(|p| {
            p.rsplit("::")
                .next()
                .is_some_and(crate::core::is_core_namespace)
        })
}

/// Literal method name from a `define_method` first argument: a symbol or
/// plain string literal names the method being defined at compile time
/// (bead ita-53y); a variable, interpolation, or call is dynamic and
/// unknowable.
pub fn literal_method_name(node: &Node<'_>) -> Option<String> {
    if let Some(s) = node.as_symbol_node() {
        return Some(String::from_utf8_lossy(s.unescaped()).into_owned());
    }
    if let Some(s) = node.as_string_node() {
        return Some(String::from_utf8_lossy(s.unescaped()).into_owned());
    }
    None
}

// ---------------------------------------------------------------------------
// Global index
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MethodSig {
    pub required: u32,
    pub optional: u32,
    pub rest: bool,
    pub keywords: Vec<(String, bool)>,
    pub kwrest: bool,
    pub sig: Option<RbsSig>,
    /// Bead ita-4xy: raw text of a preceding sorbet `sig { ... }`'s
    /// `.returns(...)` argument, copied verbatim from `MethodDef::sorbet_ret`
    /// (see that field's doc comment — same text, same "arity never reads
    /// this" rule). `None` for `#:` RBS methods, `.void` sigs, and plain
    /// unsigned defs alike; `method_return` in `check.rs` only consults it
    /// when body inference itself lands on `Ty::Unknown`.
    pub sorbet_ret: Option<String>,
    pub arity_unknown: bool,
    /// Copied from `MethodDef::abstract_stub`: the def body raises
    /// `NotImplementedError`. `check.rs` skips arity on a shadowed stub —
    /// any family member redefining the name supplies the signature that
    /// actually runs.
    pub abstract_stub: bool,
    /// Where the method is defined, for cross-file return inference.
    pub file: SourceFile,
    pub def_span: (usize, usize),
    /// Span of just the method name identifier, for go-to-definition.
    pub name_span: (usize, usize),
    /// Set only for setter methods synthesized from `db/schema.rb` (bead
    /// ita-yho): the column's raw declared type name (`"integer"`,
    /// `"date"`, ...), used by the permissive E0106 literal-cast check in
    /// `check.rs`. `None` for every ordinary method, including schema
    /// attribute readers (their type already flows through `sig`).
    pub schema_col_type: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassDef {
    pub path: String,
    /// Real lexical `Module.nesting` chain for this class (bead ita-519,
    /// see `ClassFragment::nesting`) — first fragment merged wins, same
    /// "first wins" rule as `superclass` below. Empty only for a class
    /// never seen with a real project/declared fragment (superclass and
    /// includes/prepends are then necessarily empty too, so nothing ever
    /// tries to resolve through an empty chain).
    pub nesting: Vec<String>,
    pub is_module: bool,
    pub open: bool,
    /// Why `open` is set (bead ita-anc); `None` while closed. First-reason-wins
    /// across every fragment/declaration merged into this class.
    pub open_reason: Option<OpenReason>,
    pub superclass: Option<String>,
    pub includes: Vec<String>,
    pub prepends: Vec<String>,
    pub extends: Vec<String>,
    pub methods: FxHashMap<String, MethodSig>,
    pub singleton_methods: FxHashMap<String, MethodSig>,
    pub consts: Vec<String>,
    /// Bead H of onda 2, merged from `ClassFragment`'s own three hook
    /// fields with the file each install was written in (so
    /// `apply_extended_hooks` can file a real `MethodSig`): the instance
    /// installs a `def self.extended(base)` hook performs, the singleton
    /// ones (`def base.x`), and whether that hook installed something
    /// whose name set could not be read.
    pub hook_instance_installs: Vec<(String, (usize, usize), SourceFile)>,
    pub hook_singleton_installs: Vec<(String, (usize, usize), SourceFile)>,
    pub hook_installs_opaque: bool,
    /// Table this class maps to, if it looks like an `ActiveRecord` model.
    pub table_name: Option<TableNameDecl>,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ProjectIndex {
    pub classes: Vec<ClassDef>,
    pub by_path: FxHashMap<String, ClassId>,
    /// Bead B of onda 2, staging: `run_load_hooks(:sym, Base)` bases as
    /// `(path as written, nesting at the call site)`, resolved to real
    /// `ClassId`s by `apply_load_hook_openness` once every fragment has
    /// merged (the same two-phase shape `refine_raw`/`eval_raw` use).
    load_hook_raw: Vec<(String, Vec<String>)>,
    /// Any checked file monkeypatched a core class without a fragment
    /// (bead ita-2ve): the closed-world conclusive core lookup stands
    /// down for the whole run. Absent/unset keeps v0 behavior.
    pub core_mixin: bool,
    /// Every class name any checked file refines (`refine Integer do`),
    /// merged across the project, alias-resolved, and independent of
    /// `using`'s lexical scope — see `FileDefs::refine_targets` and
    /// `resolve_refined_core`.
    pub refined_core: FxHashSet<String>,
    /// Staging for `refined_core`: raw targets plus the nesting they
    /// were written in, resolved through constant aliases once every
    /// file is merged (`resolve_refined_core`), the same two-phase shape
    /// `dynamic_mixin_raw` uses.
    refine_raw: Vec<(String, Vec<String>)>,
    /// Any checked file refined a target that could not be named — see
    /// `FileDefs::refined_unknown`.
    pub refined_unknown: bool,
    /// Every class name any checked file hands an unreadable eval body
    /// (`Integer.class_eval("def +(o) = 'x'")`), merged across the
    /// project and alias-resolved — see `FileDefs::eval_targets` and
    /// `resolve_eval_polluted_core`.
    pub eval_polluted_core: FxHashSet<String>,
    /// Staging for `eval_polluted_core`, the same two-phase shape
    /// `refine_raw` uses.
    eval_raw: Vec<(String, Vec<String>)>,
    /// Any checked file evaled a body into a target it could not name —
    /// see `FileDefs::eval_unknown`.
    pub eval_polluted_unknown: bool,
    /// Name-keyed pollution, the E0108 half of the same question: class
    /// name -> every method name some source can have added to it. Read
    /// only by `Checker::core_ops_unpolluted`; the blanket fields above
    /// still answer the closed-world core lookup, byte-identically.
    pub polluted_methods: FxHashMap<String, FxHashSet<String>>,
    /// Class names that got a source whose names could not be read (a
    /// string eval, a class-body block, an unresolvable module): any
    /// method name, this class only.
    pub polluted_opaque: FxHashSet<String>,
    /// Method names some source added to a class this project cannot
    /// name (`klass.class_eval { def zz; end }`): those names, every
    /// class. Bounded by the names — the blanket case is
    /// `polluted_unknown`.
    pub polluted_any_class: FxHashSet<String>,
    /// A source that could define ANY name on ANY class — a bare
    /// `eval(<string>)`, or an unreadable body on an unnamable receiver.
    /// Every core class stands down for every name, which is exactly
    /// today's project-wide behavior.
    pub polluted_unknown: bool,
    /// Staging for the four fields above, the same two-phase shape
    /// `refine_raw` uses: module references and constant aliases can
    /// only be chased once every file is merged
    /// (`resolve_keyed_pollution`).
    keyed_raw: Vec<(Option<String>, Vec<String>, PollutionSource)>,
    /// Direct subclasses, keyed by parent. Built once at the end of
    /// `project_index`, after every merge, so any `superclass` name that
    /// can resolve does. Exists because a self-send dispatches on the
    /// RUNTIME class: a method missing from a subclassed class may be
    /// supplied by a descendant, and claiming `NotFound` there is a false
    /// positive (see `descendant_defines`).
    pub subclasses: FxHashMap<ClassId, Vec<ClassId>>,
    /// Distinct literal `require '<lib>'` targets across every project
    /// file (W3 require/autoload). Ruby's `require` is process-global, so
    /// the stdlib gate consults this project-wide set, never per-file:
    /// a lib required in one file defines its constants for every file.
    pub requires: FxHashSet<String>,
    /// Reverse index: method name -> every project class whose OWN
    /// fragment declares it as an instance method (bead ita-dqo). Built
    /// once, at the end of `project_index` (`build_methods_by_name`,
    /// same "index once, query O(definers) later" pattern as
    /// `subclasses`/`build_subclass_map`) — never a scan of every class
    /// per call site (bead ita-9p9's standing lesson). A hit here is
    /// NOT yet a trustworthy candidate: `closed_candidates_for` filters
    /// by ancestry closedness at query time.
    pub methods_by_name: FxHashMap<String, Vec<ClassId>>,
    /// Bare constant names written at true file toplevel (`FOO = ...`
    /// outside every class/module — bead ita-exc defect B), plus the
    /// full `Owner::Name` spelling of a qualified write (`A::B = ...`)
    /// whose owner never resolved to a real project class (see
    /// `resolve_qualified_const_writes`). `DefWalker`'s
    /// `ConstantWriteNode` arm otherwise fed ONLY the resolution-inert
    /// `FileDefs::consts`/`project_consts` map (documented "never
    /// resolution") when `frag_idx` was `None` — and `const_exists`'s
    /// own bare-name widening loop structurally never reaches an empty
    /// lexical scope (`if !walked.is_empty()` guards every iteration),
    /// so a top-level constant was invisible to resolution however it
    /// was referenced. Consulted only from `const_exists`, only as the
    /// TERMINAL fallback after every lexical/ancestor check already
    /// missed: a hit here only suppresses E0104, exactly like
    /// `stdlib_declares` (invariant #1) — `resolve_const` itself never
    /// consults this and stays lexical-only.
    pub toplevel_consts: FxHashSet<String>,
    /// Bead ita-54k: constant-alias write sites (`X = Y`, or qualified
    /// `A::B = C::D`) whose RHS is ITSELF a literal constant path — full
    /// LHS path as written -> (lexical nesting at the WRITE site, RHS
    /// path as written). Resolved exactly like `resolve_const`'s own
    /// candidates (`find_const_alias`), but the RHS resolves in the
    /// ALIAS's OWN scope, not the reference site's — matching real Ruby,
    /// where the assignment's right-hand side is evaluated once, at
    /// definition time. First write wins (`entry(...).or_insert_with`),
    /// mirroring every other merge-time map here. Consulted ONLY from
    /// `const_exists` (`resolve_const_via_alias`), strictly suppression-
    /// only: `resolve_const` itself never chases an alias (see its own
    /// doc comment — widening it could manufacture a false E0101/E0102/
    /// E0103, invariant #1).
    pub const_aliases: FxHashMap<String, (Vec<String>, String)>,
    /// Bead ita-o8l.1: raw `(track, name, nesting)` entries collected
    /// from every merged file's `FileDefs::dynamic_mixin_targets`, not
    /// yet resolved to a `ClassId` — resolution needs the FULL merged
    /// `by_path` (project fragments + curated `declarations/gems.rbi` +
    /// `Gemfile.lock` namespace reopenings), so it happens exactly once,
    /// last, in `resolve_dynamic_mixin_targets`, mirroring
    /// `resolve_qualified_const_writes`'s own "resolve last" discipline.
    /// Emptied (`std::mem::take`) by that pass; never read after
    /// `project_index` returns.
    dynamic_mixin_raw: Vec<(MixinTrack, String, Vec<String>)>,
    /// Fragments produced by a BY-NAME singleton patch
    /// (`X.singleton_class.prepend M`, `class << X`), held aside by
    /// `merge_file_fragments` and drained by `apply_singleton_patches`
    /// once every file has merged — see
    /// `ClassFragment::declared_owner_required`. Never read afterwards.
    singleton_patches: Vec<(SourceFile, ClassFragment)>,
    /// Bead ita-o8l.1: resolved, deduplicated modules the project mixes
    /// in through a dynamic (non-const, non-self) receiver, split by
    /// track (`MixinTrack`) exactly like `lookup_method`/
    /// `lookup_singleton` themselves split instance vs. singleton
    /// dispatch. Consulted ONLY by `dynamic_mixin_covers`, itself called
    /// ONLY from `soften_not_found` — i.e. only after
    /// `lookup_method`/`lookup_singleton` already committed to
    /// `NotFound` on their own. See `dynamic_mixin_covers`'s doc comment
    /// for the exact suppression rule and why it replaces bead ita-a8z's
    /// rejected `Global`/`BuilderName` candidates.
    pub dynamic_mixin_instance_targets: Vec<ClassId>,
    pub dynamic_mixin_singleton_targets: Vec<ClassId>,
    /// Every merged file's `FileDefs::attributed_mixin_edges`, drained by
    /// `apply_attributed_mixin_edges` — the receiver-keyed half of the
    /// dynamic-mixin question, resolved once the full `by_path` exists.
    /// Never read after `project_index` returns.
    attributed_mixin_raw: Vec<AttributedMixinEdge>,
    /// Method name -> every `(path, nesting)` a `def` of that name
    /// provably returns (`FileDefs::const_returning_methods`), merged
    /// across the project. Keyed by NAME because the call site and the
    /// definition commonly live in different classes and different files
    /// (`AppBase#builder` calls `get_builder_class`, which only
    /// `AppGenerator` and `PluginGenerator` define) — a receiver-keyed
    /// lookup would find nothing and the whole mechanism would be inert
    /// on the very shape it exists for. Consulted only by
    /// `apply_attributed_mixin_edges`.
    const_returning_methods: FxHashMap<String, Vec<(String, Vec<String>)>>,
    /// Singleton-track, the mocking gems' class-object population:
    /// method names every class object's singleton answers to while a
    /// declared mocking gem is loaded — populated once at index build
    /// from the same `Gemfile.lock` namespaces `apply_gem_reopenings`
    /// consumes, and consulted ONLY by `soften_not_found`, itself
    /// reached only after the lookup already committed to `NotFound`.
    /// NAME-KEYED and POPULATION-GATED, never blanket: the mechanism
    /// cannot see the receiver's runtime surface, so only the name
    /// carries the proof (the same rule that replaced bead ita-a8z's
    /// receiver-blind `method_missing` clause, measured twice), and a
    /// project whose lock names neither gem keeps conclusive
    /// `NotFound` on every one of these names.
    pub mock_singleton_methods: Vec<String>,
    /// Bead ita-src: every class/module name the project defines inside a
    /// STRING literal, merged across files. Drained by
    /// `apply_string_source_definitions`; never read afterwards.
    string_source_consts: Vec<String>,
}

/// Merge all files' `file_defs` into the global class table.
#[salsa::tracked]
pub fn project_index(db: &dyn salsa::Database) -> ProjectIndex {
    let mut index = ProjectIndex::default();
    let mut qualified_writes: Vec<(String, String)> = Vec::new();
    if let Some(project) = ProjectFiles::try_get(db) {
        for &file in project.files(db) {
            merge_file_fragments(db, file, &mut index, &mut qualified_writes);
        }
        merge_schema_declarations(db, project, &mut index);
    }
    // bead ita-3gs: curated external gem declarations, embedded in the
    // binary — merged last, and only into names the project itself never
    // defined (see `merge_declared_fragment`), so `ita check`/`ita
    // server`/every test see the exact same curated set with zero extra
    // wiring, and real project code always wins over a declaration.
    for frag in &crate::declarations::declared_fragments() {
        merge_declared_fragment(&mut index, frag);
    }
    // Bead ita-547: generalizes mechanism B (bead ita-h6l) past the
    // curated set above — a gem this project's own `Gemfile.lock` names
    // (`discovery.rs`'s `GemfileLockNamespaces`, read once per process,
    // never per class here) forces open every class the project itself
    // already reopens under that gem's guessed top-level namespace. See
    // `apply_gem_reopenings`'s doc comment for the fail-closed contract.
    if let Some(gem_ns) = crate::discovery::GemfileLockNamespaces::try_get(db) {
        apply_gem_reopenings(&mut index, gem_ns.namespaces(db));
        // Singleton-track: the mocking gems' class-object surface
        // (`X.any_instance`) rides the same lock namespaces — see
        // `apply_mock_singleton_surface`'s doc comment for why the gem
        // must be in THIS project's lock and why the softening keys on
        // the method name alone.
        apply_mock_singleton_surface(&mut index, gem_ns.namespaces(db));
    }
    // Bead ita-c8h: generalizes past `apply_gem_reopenings` itself — a
    // reopening under a namespace no project file ever wrapped bare stays
    // open even when no `Gemfile.lock` gem's guessed name happens to
    // match. See `apply_undeclared_namespace_reopenings`'s doc comment.
    apply_undeclared_namespace_reopenings(&mut index);
    // Bead ita-src: a name the project only ever DEFINES inside generated
    // Ruby source (`app_file "...", <<-RUBY class Foo ... RUBY`) cannot be
    // judged from a same-named bare stub that happens to sit elsewhere in
    // the tree. Runs here, after every fragment and every namespace pass,
    // because it reads each candidate's final method maps.
    apply_string_source_definitions(&mut index);
    // Singleton-track family (c): `extend ActiveSupport::Concern` plus a
    // nested `ClassMethods` module — the idiom every Rails concern uses
    // to put class methods on its includers. Resolved here, after every
    // fragment is merged, because the nested module is commonly written
    // below (or in another file than) the `extend` that gives it its
    // meaning.
    apply_concern_class_methods(&mut index);
    // bead ita-vto (client-project `sorbet/rbi`) is deliberately NOT
    // merged here: 2.3M lines across ~1900 files at the reference corpus
    // can never be parsed eagerly on every `project_index` recompute. It
    // is instead resolved on demand, one constant at a time, from
    // `check.rs`'s own constant-resolution walk — see
    // `ProjectIndex::resolve_or_load_rbi`.
    // Bead ita-exc defect B: resolved last, after every real project
    // fragment AND every curated external declaration is merged, so an
    // owner name resolves against the full index rather than whatever
    // subset had been merged when its file was walked.
    resolve_qualified_const_writes(&mut index, qualified_writes);
    // Singleton-track step N+1, shape (1): a patch that names its target
    // (`X.singleton_class.prepend M`, `class << X`) lands only on a path
    // the project really declares, so it runs after every fragment,
    // curated declaration and gem reopening has merged.
    apply_singleton_patches(&mut index);
    // Bead ita-o8l.1: resolve every raw dynamic-mixin target collected
    // above into a real `ClassId`, once, now that every project
    // fragment, curated declaration, and gem-reopening pass has already
    // run — see `resolve_dynamic_mixin_targets`'s doc comment. Replaces
    // bead ita-a8z's env-gated `apply_a8z_candidate` (deleted): this
    // pass always runs and never opens a class, only ever feeding
    // `soften_not_found`'s NotFound->Inconclusive softening.
    resolve_dynamic_mixin_targets(&mut index);
    // Mechanism 1+2 of the attributed-mixin family: every mixin call
    // whose RECEIVER could be named, applied here — after every fragment
    // AND after `resolve_dynamic_mixin_targets`, which it mirrors, and
    // before the census/method maps below. It never resolves a method
    // and never softens a lookup by name: it opens exactly the receiver
    // classes that provably include a module answering every name (see
    // `OpenReason::MethodMissing`).
    apply_attributed_mixin_edges(&mut index);
    resolve_refined_core(&mut index);
    resolve_eval_polluted_core(&mut index);
    // Name-keyed pollution resolves LAST of the three: it is the only
    // one that reads other classes' method sets (`Module(...)`), so
    // every fragment, declaration and gem-reopening pass must already
    // have landed.
    resolve_keyed_pollution(&mut index);
    // Bead B of onda 2: `run_load_hooks(:sym, Base)` hands `Base` to a
    // block registered under that symbol, and the library runs that
    // block as `base.class_eval(&block)` — a body no fragment contains.
    //
    // Runs AFTER `resolve_keyed_pollution` deliberately.
    // `pollution_is_unreadable` (inside that pass) reads a core
    // fragment's `open_reason` to decide whether the project's own
    // ADDITIONS to a core class are enumerable, and the reason this
    // pass records is one it counts as unreadable. Opening the base
    // FIRST would therefore also stand the E0108 closed-world question
    // down for that class — silence this bead never asked for. It has
    // no business touching the pollution picture at all.
    apply_load_hook_openness(&mut index);
    // Bead H of onda 2: what a module's own `self.extended(base)` hook
    // installs on `base` lands on the EXTENDER, so this pass reads the
    // merged `extends` edges and files those installs there — after every
    // fragment, curated declaration and gem reopening has landed (the
    // module is commonly written in another file than the `extend`, and
    // `apply_concern_class_methods` above has already added its own
    // `ClassMethods` edges to the same field this pass walks).
    apply_extended_hooks(&mut index);
    build_subclass_map(&mut index);
    build_methods_by_name(&mut index);
    index
}

/// Qualified value-constant name -> where it's assigned (w12 closure):
/// feeds ONLY E0104's did-you-mean/defined-at payload — never resolution,
/// which keeps running through `ProjectIndex::const_exists`, so WHEN
/// E0104 fires is byte-identical to before this map existed. First
/// assignment wins, mirroring `merge_declared_fragment`. Deliberately
/// NOT a `ProjectIndex` field: `project_index`'s return is cloned once
/// per checked file, and a project's constant writes (1000+ at the
/// reference corpus) would ride along on every one of them; an
/// `Arc`-returning query of its own keeps that clone a refcount bump,
/// paid only by the suggestion path that reads it.
type ConstDefSites = HashMap<String, (SourceFile, (usize, usize))>;

#[salsa::tracked]
pub fn project_consts(
    db: &dyn salsa::Database,
) -> std::sync::Arc<ConstDefSites> {
    let mut out: ConstDefSites = HashMap::new();
    if let Some(project) = ProjectFiles::try_get(db) {
        for &file in project.files(db) {
            for (name, start, end) in &file_defs(db, file).consts {
                out.entry(name.clone()).or_insert((file, (*start, *end)));
            }
        }
    }
    std::sync::Arc::new(out)
}

/// Reverse the `superclass` edges once, at the end of indexing. O(n)
/// resolves here rather than a scan of every class on each miss — the
/// naive version would run on the E0101 candidate path, which is hot
/// (bead ita-9p9 is the standing reminder of what that costs).
fn build_subclass_map(index: &mut ProjectIndex) {
    let edges: Vec<(ClassId, ClassId)> = (0..index.classes.len())
        .filter_map(|i| {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "class-table index bounded by `index.classes.len()`, far below u32::MAX for any real Ruby project"
            )]
            let child = ClassId(i as u32);
            let class = &index.classes[i];
            let sup = class.superclass.as_deref()?;
            let parent = index.resolve_superclass_const(child, &class.nesting, sup)?;
            (parent != child).then_some((parent, child))
        })
        .collect();
    for (parent, child) in edges {
        index.subclasses.entry(parent).or_default().push(child);
    }
}

/// Singleton-track: the mocking gems' class-object surface, collected
/// from the same `Gemfile.lock` namespaces `apply_gem_reopenings`
/// consumes. Measured 2026-09-18 at de28b40: ALL 160 of discourse's
/// explicit-receiver residue sites are `SomeClass.any_instance` —
/// rspec-mocks (3.13.8 in discourse's lock) includes
/// `RSpec::Mocks::AnyInstance::ClassMethods` into every class and mocha
/// (3.1.0 in the same lock) monkey-patches `Object`, so while either
/// gem is loaded every class object really answers `any_instance`
/// (MRI ground truth on this machine: with `rspec/mocks` required, the
/// call returns `RSpec::Mocks::AnyInstance::Proxy`; without the gem,
/// `NoMethodError`). None of the curated declarations
/// (`declarations/gems.rbi`, the rbs collection pack) declare
/// `any_instance`/`stubs`/`expects` for rspec-mocks or mocha — the
/// declared-gem-reopening mechanism is therefore unavailable, and
/// declaring `Module#any_instance` there would force-open `Module`
/// project-wide, softening EVERY singleton lookup for EVERY project
/// (the exact blanket the task forbids). So the population is
/// lock-gated (this project's lock, keyed case-insensitively through
/// `gem_namespace_key` — `rspec-mocks` locks as `rspecmocks`) and the
/// softening is name-keyed (`soften_not_found`): a project whose lock
/// names neither gem keeps a conclusive `NotFound` on `any_instance`.
/// The measured inventory is exactly `any_instance`: zero `stubs`/
/// `expects` sites exist in any corpus residue, and rspec-mocks alone
/// does not install those names on class objects, so listing them
/// would silence real typos on rspec-only projects for no measured
/// gain.
fn apply_mock_singleton_surface(
    index: &mut ProjectIndex,
    namespaces: &std::collections::HashSet<String>,
) {
    const MOCK_GEMS: &[(&str, &[&str])] = &[("rspecmocks", &["any_instance"]), ("mocha", &["any_instance"])];
    for (key, names) in MOCK_GEMS {
        if namespaces.contains(*key) {
            for name in *names {
                if !index.mock_singleton_methods.iter().any(|m| m == name) {
                    index.mock_singleton_methods.push((*name).to_string());
                }
            }
        }
    }
}

/// Reverse index of `method name -> every project class whose OWN
/// fragment declares it` (bead ita-dqo), built once at the end of
/// `project_index` — same "build the index once, filter it cheaply per
/// query" shape as `build_subclass_map`. Candidate validity (closed
/// ancestry) is checked at query time by `closed_candidates_for`, not
/// here: a class's own ancestry can change independently of which other
/// classes define the same method name.
fn build_methods_by_name(index: &mut ProjectIndex) {
    let mut entries: Vec<(String, ClassId)> = Vec::new();
    for (i, class) in index.classes.iter().enumerate() {
        for name in class.methods.keys() {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "class-table index bounded by `index.classes.len()`, far below u32::MAX for any real Ruby project"
            )]
            entries.push((name.clone(), ClassId(i as u32)));
        }
    }
    for (name, id) in entries {
        index.methods_by_name.entry(name).or_default().push(id);
    }
}

/// One file's accumulator contributions merged into the project-wide ones:
/// the whole-index flags, the by-name raw buckets and the const-returning
/// method table. Split out of `merge_file_fragments` to stay under the
/// complexity ceiling, and named for what it is — the part of a file's
/// `file_defs` that lands in a project-wide bucket rather than on a class,
/// which is exactly the part a reader cannot check against a single class's
/// semantics.
fn merge_file_accumulators(index: &mut ProjectIndex, defs: &FileDefs) {
    index.core_mixin |= defs.core_mixin;
    index.refine_raw.extend(defs.refine_targets.iter().cloned());
    index.refined_unknown |= defs.refined_unknown;
    index.eval_raw.extend(defs.eval_targets.iter().cloned());
    index.eval_polluted_unknown |= defs.eval_unknown;
    index.keyed_raw.extend(defs.keyed_pollution.iter().cloned());
    index.dynamic_mixin_raw.extend(defs.dynamic_mixin_targets.iter().cloned());
    index.attributed_mixin_raw.extend(defs.attributed_mixin_edges.iter().cloned());
    index.load_hook_raw.extend(defs.load_hook_bases.iter().cloned());
    for name in &defs.string_source_consts {
        if !index.string_source_consts.contains(name) {
            index.string_source_consts.push(name.clone());
        }
    }
    for (method, path, nesting) in &defs.const_returning_methods {
        index
            .const_returning_methods
            .entry(method.clone())
            .or_default()
            .push((path.clone(), nesting.clone()));
    }
    index.requires.extend(defs.requires.iter().cloned());
    index.toplevel_consts.extend(defs.toplevel_consts.iter().cloned());
    // Bead ita-54k: first write wins, mirroring `toplevel_consts`/`by_path`.
    for (name, write_nesting, target) in &defs.const_aliases {
        index
            .const_aliases
            .entry(name.clone())
            .or_insert_with(|| (write_nesting.clone(), target.clone()));
    }
}

/// One file's `file_defs` merged into the global class table. Split out of
/// `project_index` to stay under the complexity ceiling (bead ita-3gs added
fn merge_file_fragments(
    db: &dyn salsa::Database,
    file: SourceFile,
    index: &mut ProjectIndex,
    qualified_writes: &mut Vec<(String, String)>,
) {
    let defs = file_defs(db, file);
    merge_file_accumulators(index, defs);
    qualified_writes.extend(defs.qualified_writes.iter().cloned());
    for frag in &defs.fragments {
        // A by-name singleton patch never interns its own path: see
        // `ClassFragment::declared_owner_required` for the TCPSocket
        // measurement that made this a separate pass.
        if frag.declared_owner_required {
            index.singleton_patches.push((file, frag.clone()));
            continue;
        }
        let id = index.intern(&frag.path);
        let class = &mut index.classes[id.0 as usize];
        class.is_module |= frag.is_module;
        class.open |= frag.open;
        // Same precedence as `open_class`: a non-AbstractRaise reason
        // always beats an existing AbstractRaise (2026-09-03, measured).
        if class.open_reason.is_none()
            || (class.open_reason == Some(OpenReason::AbstractRaise)
                && frag.open_reason.is_some_and(|r| r != OpenReason::AbstractRaise))
        {
            class.open_reason = frag.open_reason;
        }
        if class.nesting.is_empty() {
            class.nesting.clone_from(&frag.nesting);
        }
        if class.superclass.is_none() {
            class.superclass.clone_from(&frag.superclass);
        }
        if class.table_name.is_none() {
            class.table_name.clone_from(&frag.table_name);
        }
        class.includes.extend(frag.includes.iter().cloned());
        class.prepends.extend(frag.prepends.iter().cloned());
        class.extends.extend(frag.extends.iter().cloned());
        class.consts.extend(frag.consts.iter().cloned());
        for md in &frag.methods {
            class.methods.insert(md.name.clone(), method_sig(md, file));
        }
        for md in &frag.singleton_methods {
            class
                .singleton_methods
                .insert(md.name.clone(), method_sig(md, file));
        }
        merge_hook_installs(class, frag, file);
    }
}

/// Bead H of onda 2: carry one fragment's `def self.extended(base)`
/// harvest into the merged class it belongs to, remembering the file each
/// install was written in (a `MethodSig` needs it, and the install itself
/// is filed into ANOTHER class later — the extender — by
/// `apply_extended_hooks`). Shared by the two merge paths, because a
/// fragment can reach its class through either: the ordinary
/// `class`/`module` merge, and `apply_singleton_patches`' `class << X`
/// form, which walks a body that can define `X.extended` too.
fn merge_hook_installs(class: &mut ClassDef, frag: &ClassFragment, file: SourceFile) {
    for (name, span) in &frag.hook_instance_installs {
        class.hook_instance_installs.push((name.clone(), *span, file));
    }
    for (name, span) in &frag.hook_singleton_installs {
        class.hook_singleton_installs.push((name.clone(), *span, file));
    }
    class.hook_installs_opaque |= frag.hook_installs_opaque;
}

/// Bead ita-exc defect B: resolve every constant PATH write (`A::B =
/// value`) harvested by `DefWalker`'s `ConstantPathWriteNode` arm once
/// every file's fragments (and every curated external declaration) are
/// already in `index` — so `owner` resolves against the FULL project,
/// never just whichever files happened to merge before this one. A
/// literal owner that resolves lands `simple` directly on that class's
/// own `consts` bucket — the exact bucket `const_in_ancestors` already
/// reads, so no new lookup path is needed. An owner this pass can't
/// resolve (a namespace the project never declares under that literal
/// spelling, or one that only resolves via lexical scope rather than
/// the absolute text as written) falls back to `toplevel_consts`, keyed
/// by the full written path, so `const_exists`'s qualified branch still
/// has somewhere to look. Suppression-only: an unresolved owner never
/// diagnoses anything, it only misses a suppression (invariant #1).
fn resolve_qualified_const_writes(index: &mut ProjectIndex, writes: Vec<(String, String)>) {
    for (owner, simple) in writes {
        match index.by_path.get(&owner) {
            Some(&id) => index.classes[id.0 as usize].consts.push(simple),
            None => {
                index.toplevel_consts.insert(format!("{owner}::{simple}"));
            }
        }
    }
}

/// `db/schema.rb`/`db/structure.sql` attribute application (beads ita-yho,
/// ita-muf). Split out of `project_index` to stay under the complexity
/// ceiling.
fn merge_schema_declarations(
    db: &dyn salsa::Database,
    project: ProjectFiles,
    index: &mut ProjectIndex,
) {
    if let Some(schema_file) = project
        .files(db)
        .iter()
        .copied()
        .find(|f| is_schema_rb_path(f.path(db)))
    {
        let schema = crate::schema::schema_of_file(db, schema_file);
        crate::schema::apply_schema_attributes(index, schema, schema_file);
    } else if let Some(sql_project) = crate::StructureSqlProject::try_get(db) {
        // bead ita-muf: only reached when no `db/schema.rb` was found
        // above — `schema.rb` always wins when both are wired (see
        // `StructureSqlProject`'s doc comment).
        let sql_file = *sql_project.file(db);
        let schema = crate::structure_sql::structure_sql_of_file(db, sql_file);
        crate::schema::apply_schema_attributes(index, schema, sql_file);
    }
}

/// Merge one curated external declaration (bead ita-3gs, see
/// `declarations.rs`) into the index. Forces `open = true`
/// unconditionally — a declared gem namespace resolves the constant and
/// nothing else: we do not know the real gem's method surface (does it
/// have `method_missing`? does it get monkeypatched?), so any ancestry
/// running through one of these must never close (invariant #1). Mirrors
/// `schema.rs`'s "explicit project code always wins over an external
/// declaration" rule: if the project itself already defines this exact
/// path (e.g. a `config/initializers` reopening of `ActiveRecord::Base`),
/// that real definition's own `open`/methods/ancestry stand untouched —
/// the declaration only fills a name the project never defined. Methods
/// are deliberately never merged from a declaration fragment: resolving a
/// name is all a declaration is allowed to do.
fn merge_declared_fragment(index: &mut ProjectIndex, frag: &ClassFragment) {
    if index.by_path.contains_key(&frag.path) {
        return;
    }
    let id = index.intern(&frag.path);
    let class = &mut index.classes[id.0 as usize];
    class.is_module = frag.is_module;
    class.open = true;
    class.open_reason = Some(OpenReason::DeclaredExternal);
}

/// Bead ita-547: force open every class the project itself already
/// interned (a real `class`/`module` node this project wrote — never a
/// name invented here, unlike `merge_declared_fragment`) whose TOP-LEVEL
/// path segment matches one of `namespaces`. `namespaces` is
/// `GemfileLockNamespaces`'s guessed-namespace set (`discovery.rs`'s
/// `gem_namespace`), already reduced to `gem_namespace_key` form — so
/// the comparison here reduces the class path's top segment the same
/// way, making it case- and separator-insensitive. See that function's
/// doc comment for why exact-string equality was a defect and why the
/// looser SEGMENT match it could be mistaken for is refused. Read once
/// by the caller — this function never touches the filesystem or
/// re-derives the set per class.
///
/// Top-level match, not exact full-path match like
/// `is_known_external_class_path`'s curated lists: we only know a gem's
/// OWN top namespace (`Mail`), never its internal class list (`SMTP`,
/// `Message`, `Address`, ...) the way the curated `gems.rbi` allowlist
/// enumerates full paths by hand. Widening from "one exact path" to
/// "everything under this namespace" only ever produces MORE silence
/// (invariant #1's accepted false-negative direction: e.g. a project's
/// own unrelated `Mail::TestHelper` would also go open) — it can never
/// fabricate a diagnostic, so it stays inside the invariant even though
/// it is coarser than mechanism B's curated form.
fn apply_gem_reopenings(index: &mut ProjectIndex, namespaces: &std::collections::HashSet<String>) {
    if namespaces.is_empty() {
        return;
    }
    let ids: Vec<ClassId> = index
        .by_path
        .iter()
        .filter(|(path, _)| {
            namespaces.contains(&crate::discovery::gem_namespace_key(
                path.split("::").next().unwrap_or(path.as_str()),
            ))
        })
        .map(|(_, &id)| id)
        .collect();
    for id in ids {
        let class = &mut index.classes[id.0 as usize];
        class.open = true;
        // ReopenedExternal blinds lookups — it beats AbstractRaise
        // (2026-09-03, same precedence as `open_class`).
        if class.open_reason.is_none_or(|r| r == OpenReason::AbstractRaise) {
            class.open_reason = Some(OpenReason::ReopenedExternal);
        }
    }
}

/// Bead ita-c8h: structural fallback for a reopening `apply_gem_reopenings`
/// cannot see because its own top-level namespace was never (directly OR
/// transitively) named by a `Gemfile.lock` gem — measured Round-5 audit:
/// `class Rack::Attack::Request` (mastodon's `config/initializers/
/// rack_attack.rb`) reopens the `rack-attack` gem's own class, whose REAL
/// superclass (`Rack::Request`, with `ip`/`path`/`params`) is declared
/// only inside `rack` — every one of those inherited calls turned into a
/// false E0101 before this bead. `apply_gem_reopenings` happens to close
/// that specific gap already (mastodon's lock separately names the `rack`
/// gem, whose plain-camelize guess `Rack` matches `Rack::Attack::Request`'s
/// own top segment) — but that coverage is coincidental: nothing requires
/// a hyphenated gem's implied namespace segment to also be its own
/// separately-locked gem, and a project with no `Gemfile.lock` at all
/// (`GemfileLockNamespaces` absent) gets zero coverage from that mechanism.
///
/// This generalizes past gem names entirely: a project class/module path
/// with a `::` in it whose TOP-LEVEL segment was never itself the subject
/// of ANY `class`/`module` keyword this project wrote (real code, a
/// `merge_schema_declarations` attribute owner, a curated `gems.rbi`
/// declaration, or a `GemfileLockNamespaces` guess — anything already
/// merged into `by_path` by the time this runs) can only be a REOPENING of
/// a namespace defined elsewhere — the project never declared the
/// enclosing namespace, so it cannot be the one who closed this class
/// either. A bare top-level path (no `::`) is untouched: `class Foo`
/// naming a brand-new project class must keep accusing (invariant #1 — a
/// project genuinely inventing `Foo` is exactly the case this checker
/// exists to cover, and it is indistinguishable from a first-time gem
/// reopening by path shape alone).
///
/// Fail-closed by construction, not by a curated list: the ONLY way this
/// widens past `apply_gem_reopenings` is checking a bare top segment
/// against `by_path`'s full key set instead of a Gemfile-derived guess —
/// a false "this namespace is unknown" verdict only ever ADDS silence
/// (invariant #1's accepted false-negative direction), never fabricates a
/// diagnostic. Idiomatic Rails-generator style (`module Admin; class
/// FooController < BaseController; ...; end; end`, one file per
/// controller) already interns the bare namespace (`Admin`) as its own
/// fragment the first time ANY file wraps it that way, so a project using
/// that convention is unaffected — only a namespace NO file ever wraps
/// bare gets treated as external. Run after `apply_gem_reopenings` (order
/// does not matter for correctness — both are additive-only and respect
/// first-reason-wins — but this is the more general, coarser-grained
/// fallback, so it reads naturally as the last resort).
fn apply_undeclared_namespace_reopenings(index: &mut ProjectIndex) {
    let ids: Vec<ClassId> = index
        .by_path
        .iter()
        .filter(|(path, _)| match path.split_once("::") {
            // A top segment present ONLY because a curated declarations
            // file names it (`OpenReason::DeclaredExternal`) is not the
            // project declaring it — bead ita-dpg.1's generated pack put
            // hundreds of gem namespaces into `by_path`, which would
            // otherwise silently shrink this fallback for every project
            // class nested under one of them (caught by
            // `gem_reopen_lockfile.rs`).
            Some((top, _rest)) => index.by_path.get(top).is_none_or(|&id| {
                index.classes[id.0 as usize].open_reason == Some(OpenReason::DeclaredExternal)
            }),
            None => false,
        })
        .map(|(_, &id)| id)
        .collect();
    for id in ids {
        let class = &mut index.classes[id.0 as usize];
        class.open = true;
        // ReopenedExternal blinds lookups — it beats AbstractRaise
        // (2026-09-03, same precedence as `open_class`).
        if class.open_reason.is_none_or(|r| r == OpenReason::AbstractRaise) {
            class.open_reason = Some(OpenReason::ReopenedExternal);
        }
    }
}

/// Is this an `extend ActiveSupport::Concern` edge, spelled with or
/// without the leading cbase? The one predicate both readers of the
/// concern idiom share: `DefWalker`'s `class_methods do` harvest gate
/// (harvest-time, same-file edge) and `apply_concern_class_methods`
/// (post-merge, any-file edge). Both must ask the SAME question or the
/// two spellings of the idiom drift apart.
fn is_concern_edge(extends: &[String]) -> bool {
    extends.iter().any(|e| e.trim_start_matches("::") == "ActiveSupport::Concern")
}

/// Singleton-track family (c): `ActiveSupport::Concern`'s `ClassMethods`
/// convention. A module that `extend ActiveSupport::Concern` and defines
/// a nested `ClassMethods` module has that module extended onto EVERY
/// includer's singleton by the gem's own `append_features` — the shape
/// behind `AdminDashboardIndexData.fetch_cached_stats` (discourse,
/// `StatsCacheable::ClassMethods`, 4 measured residue sites) and behind
/// the concern half of `singleton_lookup.rs`'s characterized gap.
///
/// Modeled as an `extend <own path>::ClassMethods` edge on the concern
/// itself, which is all `lookup_singleton` needs: it already walks an
/// ancestor's `extends` and reads the extended module's `methods` map
/// onto the receiver's singleton, and a concern is an ordinary include
/// ancestor of its includers. This runs as a post-pass, after every
/// fragment is merged, because `module ClassMethods` is normally written
/// below the `extend` that gives it its meaning — resolving it during
/// the walk would depend on statement order.
///
/// Three deliberate limits. It requires the NESTED path to exist in the
/// index (`{concern}::ClassMethods`): no `ClassMethods`, no edge, never
/// a guess. It keys on the literal `ActiveSupport::Concern` edge
/// (`::`-prefixed or not), never on "some name ending in Concern" — a
/// project's own `Concern`-suffixed module is not this gem's protocol.
/// And it over-approximates in exactly one direction: the concern module
/// OBJECT itself gains the names too, though MRI only gives them to
/// includers. That can only resolve a call whose runtime is already
/// `NoMethodError`, never hide one and never fabricate a diagnostic
/// (invariant #1).
fn apply_concern_class_methods(index: &mut ProjectIndex) {
    let edges: Vec<(ClassId, String)> = index
        .classes
        .iter()
        .enumerate()
        .filter(|(_, class)| is_concern_edge(&class.extends))
        .filter_map(|(i, class)| {
            let nested = format!("{}::ClassMethods", class.path);
            let id = ClassId(u32::try_from(i).ok()?);
            index.by_path.contains_key(&nested).then_some((id, nested))
        })
        .collect();
    for (id, nested) in edges {
        let class = &mut index.classes[id.0 as usize];
        if !class.extends.contains(&nested) {
            class.extends.push(nested);
        }
    }
}

/// Bead B of onda 2: mark every `run_load_hooks(:sym, Base)` base `open`
/// — its instance surface is whatever the block registered under
/// `:sym` installs when the library calls `base.class_eval(&block)`
/// (`activesupport/lib/active_support/lazy_load_hooks.rb:107`), and this
/// index can reach that body through NO fragment: `on_load` stores the
/// block under a symbol in a hash, `run_load_hooks` looks it up at
/// runtime, and the hook's own receiver is a method parameter (`def
/// run_load_hooks(name, base = Object)`), so neither the block nor the
/// base is attributed to anything.
///
/// `Open` with `OpenReason::EvalOrSend` is the exact and only honest
/// answer: this checker knows a body it never read was `class_eval`'d
/// into that class, so every miss on it is `Inconclusive` (silence) and
/// never `NotFound`. The reused reason is not a new variant because the
/// census's `tally_ancestry` matches `OpenReason` exhaustively, and this
/// shape IS the `EvalOrSend` family (`class_eval` of a body nobody
/// showed the checker) — see that variant's own doc comment.
///
/// Two-phase, like `resolve_dynamic_mixin_targets`: the base is resolved
/// against the FULL merged index, because `Base` commonly lives in
/// another file than the `run_load_hooks` call site. A base that does
/// not resolve contributes nothing — fail-closed (invariant #1), never a
/// widened match.
fn apply_load_hook_openness(index: &mut ProjectIndex) {
    let raw = std::mem::take(&mut index.load_hook_raw);
    for (base, nesting) in raw {
        let Some(id) = index.resolve_const(&nesting, &base) else { continue };
        merge_open(index, id, OpenReason::EvalOrSend);
    }
}

/// Mark class `id` open for `reason` from a merge-time pass, with
/// `merge_file_fragments`' own precedence: an existing reason is kept,
/// except `AbstractRaise` — the WEAKEST of them (an abstract stub is
/// treated as closed and only softens through the receiver's own
/// subtree), which any later, stronger open must replace. Same rule
/// `open_class` and `apply_attributed_mixin_edges` apply.
fn merge_open(index: &mut ProjectIndex, id: ClassId, reason: OpenReason) {
    let class = &mut index.classes[id.0 as usize];
    class.open = true;
    if class.open_reason.is_none() || class.open_reason == Some(OpenReason::AbstractRaise) {
        class.open_reason = Some(reason);
    }
}

/// Bead H of onda 2: apply every module's `def self.extended(base)`
/// harvest to the classes that really `extend` it — the instance installs
/// onto the extender's INSTANCE surface (where `base.delegate`'s methods
/// run), the `def base.x` names onto its SINGLETON surface, and the
/// fail-closed open when the hook body could not be read.
///
/// Merge-time, after every fragment (and every curated declaration and gem
/// reopening) has landed: the module a class extends is commonly written
/// in another file, and the extender is only knowable from the merged
/// `extends` edges — the same two-phase shape
/// `resolve_dynamic_mixin_targets` uses.
///
/// Additive and arity-inert only: every install is filed with
/// `arity_unknown`, so this can turn an existing `NotFound` into `Found`
/// (silence) and can never manufacture an E0102. An install never
/// overwrites a method the extender's own body defines — the same
/// first-wins rule every merge here uses.
fn apply_extended_hooks(index: &mut ProjectIndex) {
    let mut edges: Vec<(ClassId, ClassId)> = Vec::new();
    for (i, class) in index.classes.iter().enumerate() {
        for ext in &class.extends {
            if let Some(module) = index.resolve_const(&class.nesting, ext) {
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "class-table index bounded by `self.classes.len()`, far below u32::MAX for any real Ruby project"
                )]
                edges.push((ClassId(i as u32), module));
            }
        }
    }
    for (extender, module) in edges {
        apply_one_extended_hook(index, extender, module);
    }
}

/// One `(extender, module)` edge of `apply_extended_hooks`: file what the
/// module's hook installs on `base` — see that function's contract. Split
/// out to stay under the complexity ceiling, and named for the single edge
/// it resolves.
fn apply_one_extended_hook(index: &mut ProjectIndex, extender: ClassId, module: ClassId) {
    let (instance, singleton, opaque) = {
        let m = index.class(module);
        (
            m.hook_instance_installs.clone(),
            m.hook_singleton_installs.clone(),
            m.hook_installs_opaque,
        )
    };
    let class = &mut index.classes[extender.0 as usize];
    for (name, span, file) in instance {
        class
            .methods
            .entry(name)
            .or_insert_with(|| hook_install_sig(span, file));
    }
    for (name, span, file) in singleton {
        class
            .singleton_methods
            .entry(name)
            .or_insert_with(|| hook_install_sig(span, file));
    }
    if opaque {
        merge_open(index, extender, OpenReason::EvalOrSend);
    }
}

/// A method a `def self.extended(base)` hook installed on its extender.
/// Only its NAME and where it was written are known: `delegate`'s own
/// `def name(*args, &block)` and a `define_method` body both take an
/// arity this index never sees, so the sig is arity-inert
/// (`arity_unknown` — the same skip `check_arity` applies to a `rest`
/// method) and can never manufacture an E0102.
fn hook_install_sig(span: (usize, usize), file: SourceFile) -> MethodSig {
    MethodSig {
        required: 0,
        optional: 0,
        rest: false,
        keywords: Vec::new(),
        kwrest: false,
        sig: None,
        sorbet_ret: None,
        arity_unknown: true,
        abstract_stub: false,
        file,
        def_span: span,
        name_span: span,
        schema_col_type: None,
    }
}

/// Bead ita-src: every `class X`/`module X` name written in `body`, a
/// STRING literal's content. Deliberately a text scan and not a
/// sub-parse: AGENTS.md already records that parsing literal eval bodies
/// as sub-programs was measured and rejected, and the only question here
/// is "does the project generate a definition of this NAME", which the
/// keyword plus the first constant segment answers.
fn collect_string_source_consts(body: &str, out: &mut Vec<String>) {
    for line in body.lines() {
        let line = line.trim_start();
        let Some(rest) = line
            .strip_prefix("class ")
            .or_else(|| line.strip_prefix("module "))
            .map(str::trim_start)
        else {
            continue;
        };
        let name: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if name.starts_with(|c: char| c.is_ascii_uppercase()) && !out.contains(&name) {
            out.push(name);
        }
    }
}

/// Bead ita-esc: how many times the local named `name` is READ anywhere
/// inside `node`. `harvest_extended_hook` compares this with the reads
/// its shallow walk consumed: a surplus means the hook's `base` reached
/// code that walk never modelled.
fn count_local_reads(node: &Node<'_>, name: &str) -> usize {
    struct Counter<'a> {
        name: &'a str,
        n: usize,
    }
    impl<'pr> Visit<'pr> for Counter<'_> {
        fn visit_local_variable_read_node(
            &mut self,
            node: &ruby_prism::LocalVariableReadNode<'pr>,
        ) {
            if String::from_utf8_lossy(node.name().as_slice()) == self.name {
                self.n += 1;
            }
        }
    }
    let mut c = Counter { name, n: 0 };
    c.visit(node);
    c.n
}

/// Bead ita-blk: collects every `class X`/`module X` keyword reachable
/// from a class-body block, carrying the LEXICAL scope Ruby gives it.
/// A `class` keyword's cref is lexical, never the block's runtime `self`,
/// so the path this builds is the one MRI defines; recursing through the
/// definition's own body keeps that true for nested spellings.
struct NestedDefScan<'a> {
    /// Innermost-last lexical scope; index 0 is the enclosing class body
    /// the block was written in.
    scope: Vec<String>,
    /// `Module.nesting` of that enclosing class body, extended as the
    /// scan descends.
    nesting: Vec<String>,
    /// `(full path, nesting, is_module)` per definition found.
    out: &'a mut Vec<(String, Vec<String>, bool)>,
}

impl NestedDefScan<'_> {
    fn enter(&mut self, path: Option<String>, is_module: bool) -> bool {
        let Some(path) = path else { return false };
        let full = join_path(self.scope.last().map_or("", String::as_str), &path);
        self.nesting.push(full.clone());
        self.out.push((full.clone(), self.nesting.clone(), is_module));
        self.scope.push(full);
        true
    }

    fn leave(&mut self, entered: bool) {
        if entered {
            self.scope.pop();
            self.nesting.pop();
        }
    }
}

impl<'pr> Visit<'pr> for NestedDefScan<'_> {
    fn visit_class_node(&mut self, node: &ruby_prism::ClassNode<'pr>) {
        let entered = self.enter(const_path_str(&node.constant_path()), false);
        ruby_prism::visit_class_node(self, node);
        self.leave(entered);
    }

    fn visit_module_node(&mut self, node: &ruby_prism::ModuleNode<'pr>) {
        let entered = self.enter(const_path_str(&node.constant_path()), true);
        ruby_prism::visit_module_node(self, node);
        self.leave(entered);
    }
}

/// Is `node` a bare read of the `def self.extended(base)` hook's own
/// parameter (`base`)? Only that name is the base: a read of any other
/// local in the hook body names someone else, and filing its calls here
/// would attribute another object's methods to the extender.
fn hook_param_read(node: &Node<'_>, pname: &str) -> bool {
    node.as_local_variable_read_node()
        .is_some_and(|read| String::from_utf8_lossy(read.name().as_slice()) == pname)
}

/// The names `<base>.delegate` installs when it is called ON the base,
/// read off the call's own argument list. `None` means the set is
/// unreadable, and the caller must fail closed.
fn hook_delegate_names(call: &ruby_prism::CallNode<'_>) -> Option<Vec<(String, (usize, usize))>> {
    let mut out = Vec::new();
    for arg in &call.arguments()?.arguments() {
        if let Some(sym) = arg.as_symbol_node() {
            out.push((
                String::from_utf8_lossy(sym.unescaped()).into_owned(),
                span_of(&arg),
            ));
            continue;
        }
        if delegate_arg_unreadable(&arg) {
            return None;
        }
    }
    Some(out)
}

/// Is this non-symbol `delegate` argument one that makes the installed
/// NAME SET unreadable? `prefix:`/`suffix:` rewrite the names
/// (`delegate :name, to: :class, prefix: true` installs `class_name`), and
/// a splat, a variable, or any other expression names methods only at
/// runtime. The keywords every other spelling takes (`to:`, `allow_nil:`,
/// `private:`, `public:`) leave the name alone.
fn delegate_arg_unreadable(arg: &Node<'_>) -> bool {
    const NAME_KEEPING: [&str; 4] = ["to", "allow_nil", "private", "public"];
    let Some(kw) = arg.as_keyword_hash_node() else { return true };
    !kw.elements().iter().all(|el| {
        el.as_assoc_node().is_some_and(|assoc| {
            assoc.key().as_symbol_node().is_some_and(|k| {
                NAME_KEEPING.contains(&String::from_utf8_lossy(k.unescaped()).as_ref())
            })
        })
    })
}

/// The name a `define_method` call installs, when its first argument is a
/// literal symbol or string; `None` (a dynamic name) is the caller's
/// fail-closed signal.
fn hook_define_method_name(call: &ruby_prism::CallNode<'_>) -> Option<String> {
    call.arguments()
        .and_then(|a| a.arguments().iter().next())
        .and_then(|a| literal_method_name(&a))
}

/// Bead ita-o8l.1: resolve every `(track, name, nesting)` entry
/// collected project-wide (`ProjectIndex::dynamic_mixin_raw`) into a
/// `ClassId`, once, after every fragment — real project source, curated
/// `declarations/gems.rbi`, `Gemfile.lock` namespace reopenings — is
/// already merged into `by_path`, exactly like
/// `resolve_qualified_const_writes`'s own "resolve last, against the
/// full index" discipline (a dynamically-mixed module's OWN definition
/// commonly lives in a different file than the `include`/`extend`/
/// `prepend` call site — `Rails::ActionMethods`/`app_base.rb`'s own
/// canonical example). A name that never resolves anywhere in the
/// project (a pure-gem module this checker has no fragment for at all)
/// is simply dropped: a false negative — this checker can't see the
/// module's methods either way — never treated as a signal to widen
/// anything else (invariant #1).
fn resolve_dynamic_mixin_targets(index: &mut ProjectIndex) {
    for (track, name, nesting) in std::mem::take(&mut index.dynamic_mixin_raw) {
        if let Some(id) = index.resolve_const(&nesting, &name) {
            match track {
                MixinTrack::Instance => index.dynamic_mixin_instance_targets.push(id),
                MixinTrack::Singleton => index.dynamic_mixin_singleton_targets.push(id),
            }
        }
    }
    index.dynamic_mixin_instance_targets.sort_unstable();
    index.dynamic_mixin_instance_targets.dedup();
    index.dynamic_mixin_singleton_targets.sort_unstable();
    index.dynamic_mixin_singleton_targets.dedup();
}

/// Mechanisms 1 and 2 of the attributed-mixin family: apply every mixin
/// call whose RECEIVER could be named, by opening exactly that receiver
/// when the module it provably includes defines `method_missing` /
/// `respond_to_missing?`.
///
/// Why this and not a wider rule. Bead ita-a8z measured — twice, in
/// `AGENTS.md` — that keying a dynamic-mixin softening on "some mixed
/// module has `method_missing`" silences EVERY `NotFound` on that track
/// project-wide: 222/222 of rails' baseline E0101 and 207/207 of a
/// private corpus's, because a real project always contains some
/// unrelated dynamically-mixed `method_missing` target. The fix is not a
/// narrower predicate, it is a keyed one: the openness follows the
/// RECEIVER. `builder_class.include(ActionMethods)` names no receiver at
/// the call site, but its value is a class this index can prove
/// (`MixinReceiver`), and once it is proven the rule is exactly the one
/// `DefWalker` already applies to a class whose OWN body defines
/// `method_missing` (`index.rs`'s `DefNode` arm): that class answers
/// every name, so it is `open`.
///
/// Fail-closed at every step: a receiver that does not resolve, a module
/// that does not resolve, and a module whose `methods` map has neither
/// `method_missing` nor `respond_to_missing?` each contribute nothing.
/// The module's OWN map only, never its ancestry — the same deliberate
/// shallowness `dynamic_mixin_covers` documents, and for the same reason
/// (a second unbounded chase through an arbitrary module's own mixins
/// silences far more than any measurement here justifies).
///
/// ADDITIVE ONLY: a class already `open` for another reason keeps that
/// reason (`AbstractRaise` excepted, exactly as `apply_singleton_patches`
/// does — an abstract stub is the WEAKEST reason and a real
/// `method_missing` must replace it).
fn apply_attributed_mixin_edges(index: &mut ProjectIndex) {
    let raw = std::mem::take(&mut index.attributed_mixin_raw);
    for rid in attributed_mixin_receivers(index, raw) {
        let class = &mut index.classes[rid.0 as usize];
        class.open = true;
        // `AbstractRaise` is the weakest reason (an abstract stub is
        // treated as closed and only softens through the receiver's
        // subtree) — a real `method_missing` must REPLACE it, exactly as
        // `open_class` does.
        if class.open_reason.is_none() || class.open_reason == Some(OpenReason::AbstractRaise) {
            class.open_reason = Some(OpenReason::MethodMissing);
        }
    }
}

/// The receivers an attributed mixin edge PROVES: the classes each edge
/// names, resolved, kept only where the module it mixes in really answers
/// every name. Fail-closed at every step — a receiver that does not
/// resolve, a module that does not resolve, and a module whose OWN map has
/// neither `method_missing` nor `respond_to_missing?` each contribute
/// nothing. Split out of `apply_attributed_mixin_edges` to stay under the
/// complexity ceiling, and named for the half it is: which receivers the
/// edges prove, before anything is opened.
fn attributed_mixin_receivers(
    index: &ProjectIndex,
    raw: Vec<AttributedMixinEdge>,
) -> Vec<ClassId> {
    let mut receivers: Vec<ClassId> = Vec::new();
    for edge in raw {
        let candidate_paths: Vec<(String, Vec<String>)> = match &edge.receiver {
            MixinReceiver::Path { path, nesting } => vec![(path.clone(), nesting.clone())],
            MixinReceiver::Call { method } => {
                index.const_returning_methods.get(method).cloned().unwrap_or_default()
            }
        };
        if candidate_paths.is_empty() {
            continue;
        }
        let Some(mid) = index.resolve_const(&edge.module_nesting, &edge.module) else { continue };
        let module = index.class(mid);
        if !module.methods.contains_key("method_missing")
            && !module.methods.contains_key("respond_to_missing?")
        {
            continue;
        }
        for (path, nesting) in candidate_paths {
            if let Some(rid) = index.resolve_const(&nesting, &path) {
                receivers.push(rid);
            }
        }
    }
    receivers.sort_unstable();
    receivers.dedup();
    receivers
}

/// Bead ita-src: open every BARE STUB whose name the project also writes
/// as a `class X`/`module X` inside a string literal.
///
/// Two conditions, both required, and the conjunction is what keeps this
/// from being a blanket. (1) NAME: some file generates Ruby source
/// defining that name, so the parsed tree is not where the real
/// definition lives. (2) SHAPE: the fragment this project does parse has
/// no methods and no singleton methods of its own — a placeholder, never
/// a class whose surface anybody could check. rails'
/// `RaisesNoMethodError` fixture passes (2) and fails (1): no string
/// literal in the tree defines that name, so its deliberate
/// `NoMethodError` stays conclusive. The `Foo` this closes fails neither
/// (`railties/test/application/configuration_test.rb:5352` writes
/// `class Foo < ApplicationRecord` into a generated model file, and the
/// only parsed top-level `Foo` is an empty stub in another sub-gem's
/// tests).
fn apply_string_source_definitions(index: &mut ProjectIndex) {
    let names = std::mem::take(&mut index.string_source_consts);
    if names.is_empty() {
        return;
    }
    let mut targets: Vec<ClassId> = Vec::new();
    for (i, class) in index.classes.iter().enumerate() {
        if !class.methods.is_empty() || !class.singleton_methods.is_empty() {
            continue;
        }
        let last = class.path.rsplit("::").next().unwrap_or(&class.path);
        if names.iter().any(|n| n == last) {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "class-table index bounded by `self.classes.len()`, far below u32::MAX for any real Ruby project"
            )]
            targets.push(ClassId(i as u32));
        }
    }
    for id in targets {
        merge_open(index, id, OpenReason::StringSourceDefined);
    }
}

/// Singleton-track step N+1, shape (1): apply every held-aside by-name
/// singleton patch (`X.singleton_class.prepend M`, `class << X`) to the
/// class it names — and ONLY if the project already declares that class.
///
/// `by_path.get`, never `intern`: interning here is the whole defect
/// this pass exists to avoid. Measured 2026-09-17 on discourse, with
/// the naive version that interned: `TCPSocket.singleton_class.prepend`
/// in `lib/freedom_patches/final_destination_connect.rb` invented a
/// `TCPSocket` class, which then looked like a CLOSED project class
/// with no methods at all, and `TCPSocket.new(...).close` in
/// `spec/support/nginx_test_proxy.rb:145` became a new `E0101` on code
/// that runs — invariant #1, from a pass whose only job was to add
/// knowledge. A patch naming a stdlib or gem class is dropped: the
/// checker cannot see that class's methods either way, so the
/// false-negative direction is the correct cost.
///
/// What lands: the singleton `extends` edges, the singleton methods the
/// `class << X` body defined, and any openness that body produced.
/// Instance-track content cannot reach here — the walker only ever
/// writes these fragments with `in_singleton` true or as an `extends`
/// edge — so nothing about instance dispatch changes.
fn apply_singleton_patches(index: &mut ProjectIndex) {
    for (file, frag) in std::mem::take(&mut index.singleton_patches) {
        let Some(&id) = index.by_path.get(&frag.path) else { continue };
        let class = &mut index.classes[id.0 as usize];
        // Before the `extends` loop below moves `frag.extends` out.
        merge_hook_installs(class, &frag, file);
        for path in frag.extends {
            if !class.extends.contains(&path) {
                class.extends.push(path);
            }
        }
        for md in &frag.singleton_methods {
            class
                .singleton_methods
                .entry(md.name.clone())
                .or_insert_with(|| method_sig(md, file));
        }
        if frag.open {
            class.open = true;
            if class.open_reason.is_none()
                || (class.open_reason == Some(OpenReason::AbstractRaise)
                    && frag.open_reason.is_some_and(|r| r != OpenReason::AbstractRaise))
            {
                class.open_reason = frag.open_reason;
            }
        }
    }
}

/// Turn every raw refinement target (`ProjectIndex::refine_raw`) into the
/// class NAMES `Checker::core_class_unpolluted` compares against, once
/// every file is merged so constant aliases can be chased.
///
/// Two names can come out of one target, and both are recorded because
/// either spelling may be the one a pollution list carries: the target as
/// written (`refine Integer` -> `Integer`, and a project constant like
/// `Foo::Bar` -> `Foo::Bar`, which matches no core name and so poisons
/// nothing), plus — when the target is a constant ALIAS whose chain
/// leaves the project (`I = Integer`) — the alias's final target text
/// (`Integer`), via `expand_unresolved_alias_target`. Round-4 review
/// measured `I = Integer; refine I do def +(o) ... end end` accused on a
/// program MRI runs clean: the raw spelling `I` matched no core name.
fn resolve_refined_core(index: &mut ProjectIndex) {
    for (name, nesting) in std::mem::take(&mut index.refine_raw) {
        index.refined_core.insert(name.clone());
        if let Some(expanded) = index.expand_unresolved_alias_target(&nesting, &name) {
            index.refined_core.insert(expanded.trim_start_matches("::").to_string());
        }
    }
}

/// Turn every raw eval-pollution target (`ProjectIndex::eval_raw`) into
/// the class NAMES `Checker::core_class_unpolluted` compares against,
/// once every file is merged so constant aliases can be chased — the same
/// two-phase shape and the same two recorded spellings as
/// `resolve_refined_core` above, and for the same measured reason:
/// `I = Integer; I.class_eval("def +(o) = 'aliased'")` prints
/// `"aliased"` under MRI, and the raw spelling `I` matches no core name.
fn resolve_eval_polluted_core(index: &mut ProjectIndex) {
    for (name, nesting) in std::mem::take(&mut index.eval_raw) {
        index.eval_polluted_core.insert(name.clone());
        if let Some(expanded) = index.expand_unresolved_alias_target(&nesting, &name) {
            index.eval_polluted_core.insert(expanded.trim_start_matches("::").to_string());
        }
    }
}

/// Turn every raw name-keyed source (`ProjectIndex::keyed_raw`) into the
/// four maps `Checker::core_ops_unpolluted` reads, once every file is
/// merged: only then can a `Module(...)` reference be resolved to a real
/// method set, and only then can a constant alias be chased.
///
/// Both spellings of a target are recorded, exactly as
/// `resolve_refined_core` does and for the same measured reason
/// (`I = Integer; I.class_eval { def +(o) = 'x' }` runs clean under
/// MRI). Targets that name no core class are dropped here rather than
/// stored: the map would otherwise carry every project class in the
/// repository, and E0108 only ever asks about the fourteen core names.
fn resolve_keyed_pollution(index: &mut ProjectIndex) {
    for (target, nesting, source) in std::mem::take(&mut index.keyed_raw) {
        let names: Vec<String> = match &source {
            PollutionSource::Names(n) => n.clone(),
            PollutionSource::Opaque => Vec::new(),
            // An unresolvable module — a gem's, a `DeclaredExternal`
            // namespace's, or one this project reopens dynamically — can
            // carry any method at all.
            PollutionSource::Module(path) => {
                if let Some(n) = module_method_names(index, &nesting, path) {
                    n
                } else {
                    mark_opaque(index, target.as_ref(), &nesting);
                    continue;
                }
            }
        };
        if matches!(source, PollutionSource::Opaque) {
            mark_opaque(index, target.as_ref(), &nesting);
            continue;
        }
        match target {
            None => index.polluted_any_class.extend(names),
            Some(t) => {
                for spelling in target_spellings(index, &nesting, &t) {
                    index
                        .polluted_methods
                        .entry(spelling)
                        .or_default()
                        .extend(names.iter().cloned());
                }
            }
        }
    }
    resolve_fragment_pollution(index);
}

/// The reopening source: every FRAGMENT whose name is a core class, read
/// off the merged index rather than off any one file's AST.
///
/// A second reader over the class BODY was written first and then deleted,
/// measured: every shape it could see, this pass already sees, because
/// the walker attributes the same statements to the same fragment — a
/// `def` and a literal `define_method`/`attr_*`/`alias_method` land in
/// `methods`, and every shape it cannot read opens the class with a
/// reason `pollution_is_unreadable` treats as `Opaque`. The mutation
/// matrix is what settled it: with the body reader removed the whole
/// suite stayed green (`scripts/operand-types-mutants.sh`, round 6), and
/// a mechanism no test can distinguish is not a safeguard. Reading the
/// merged fragment also buys the shape the body reader could NOT see:
/// a reopening through a constant ALIAS (`I = Integer; class I; def
/// +(o) = 1; end` prints nothing and exits 0 under MRI), since the file
/// scan would have had to collect every project class's body to catch
/// `I`.
fn resolve_fragment_pollution(index: &mut ProjectIndex) {
    let mut readable: Vec<(String, Vec<String>)> = Vec::new();
    let mut opaque: Vec<String> = Vec::new();
    let mut modules: Vec<(String, String)> = Vec::new();
    for (id, spelling) in core_fragment_candidates(index) {
        let class = &index.classes[id.0 as usize];
        // Not `class.open` — the REASON, measured: every reopening of a
        // core class is `open` by construction
        // (`OpenReason::ReopenedExternal`, because the class is defined
        // outside the project), which read as "unreadable" silenced even
        // `class Integer; def zz; end`. `open` answers "do we know this
        // class's WHOLE surface"; pollution asks "do we know what this
        // project ADDED", and only the reasons `pollution_is_unreadable`
        // lists are evidence against that.
        if pollution_is_unreadable(class.open_reason) {
            opaque.push(spelling);
            continue;
        }
        readable.push((
            spelling.clone(),
            class
                .methods
                .keys()
                .chain(class.singleton_methods.keys())
                .cloned()
                .collect(),
        ));
        for m in class.includes.iter().chain(&class.prepends) {
            modules.push((spelling.clone(), m.clone()));
        }
    }
    for name in opaque {
        index.polluted_opaque.insert(name);
    }
    for (name, methods) in readable {
        index.polluted_methods.entry(name).or_default().extend(methods);
    }
    for (name, module) in modules {
        match module_method_names(index, &[], &module) {
            Some(n) => index.polluted_methods.entry(name).or_default().extend(n),
            None => {
                index.polluted_opaque.insert(name);
            }
        }
    }
}

/// Every fragment that reopens a core class, as `(fragment, the core name
/// it pollutes)`.
///
/// Inverted on purpose: ask the fourteen core names whether the project
/// reopened them, plus the (few) constant aliases, instead of asking
/// every fragment in the repository whether it is a core name. The
/// straightforward direction cost 19% of `check/project_index` (7.19 ms
/// against a 7.04 ms ceiling, paired against 6.04 ms on the parent
/// revision in the same window) because `target_spellings` chases an
/// alias, and an alias chase per project class is a lookup per class.
fn core_fragment_candidates(index: &ProjectIndex) -> Vec<(ClassId, String)> {
    let mut out: Vec<(ClassId, String)> = Vec::new();
    for name in crate::core::core_namespace_names() {
        if let Some(&id) = index.by_path.get(*name) {
            out.push((id, (*name).to_string()));
        }
    }
    for alias in index.const_aliases.keys() {
        let Some(&id) = index.by_path.get(alias) else { continue };
        for spelling in target_spellings(index, &[], alias) {
            out.push((id, spelling));
        }
    }
    out
}

/// Does this `open_reason` mean "the project added methods here that
/// this checker cannot name"? The name-keyed pollution question, which
/// is not the closed-world question `open` itself answers.
///
/// `Yes` for every shape that runs unreadable code at definition time.
/// `No` for the reasons that say only "this class lives outside the
/// project" (`ReopenedExternal`, `DeclaredExternal`, `DynamicSuperclass`)
/// — those are why a core reopening is open AT ALL, and reading them as
/// unreadable makes the whole name-keying inert. `MethodMissing` is
/// `No` on purpose and loses nothing: the method is in the class's own
/// `methods` map, and `method_missing` is in every key set.
fn pollution_is_unreadable(reason: Option<OpenReason>) -> bool {
    match reason {
        Some(
            OpenReason::DynamicMixinReceiver
            | OpenReason::DynamicMixinArg
            | OpenReason::DynamicAttrArg
            | OpenReason::DynamicDefineMethod
            | OpenReason::DynamicAliasMethod
            | OpenReason::EvalOrSend
            | OpenReason::UnknownClassBodyCall
            | OpenReason::ClassBodyBlock
            | OpenReason::SingletonClassExpr
            | OpenReason::AbstractRaise
            | OpenReason::Unattributed
            | OpenReason::NestedDefOwner
            | OpenReason::BlockNestedDefinition
            | OpenReason::StringSourceDefined,
        ) => true,
        Some(
            OpenReason::ReopenedExternal
            | OpenReason::DeclaredExternal
            | OpenReason::DynamicSuperclass
            | OpenReason::MethodMissing,
        )
        | None => false,
    }
}

/// Both recorded spellings of a keyed target, filtered to core names —
/// see `resolve_keyed_pollution`.
fn target_spellings(index: &ProjectIndex, nesting: &[String], target: &str) -> Vec<String> {
    let mut out = Vec::new();
    if crate::core::is_core_namespace(target) {
        out.push(target.to_string());
    }
    // Two chasers, because a REOPENING through an alias resolves in
    // project after all and `expand_unresolved_alias_target` bails on
    // exactly that: `I = Integer; class I; def +(o) = 1; end` creates a
    // fragment named `I`, so the alias has to be read from the alias
    // table itself. Measured: that program prints nothing and exits 0
    // under MRI, and it accused until this line existed.
    let chased = index
        .expand_unresolved_alias_target(nesting, target)
        .or_else(|| index.chase_alias_target_text(nesting, target));
    if let Some(expanded) = chased {
        let expanded = expanded.trim_start_matches("::").to_string();
        if crate::core::is_core_namespace(&expanded) && !out.contains(&expanded) {
            out.push(expanded);
        }
    }
    out
}

/// Record "this class got a body we cannot read": one class if it can be
/// named, every class for every name if it cannot.
fn mark_opaque(index: &mut ProjectIndex, target: Option<&String>, nesting: &[String]) {
    match target {
        None => index.polluted_unknown = true,
        Some(t) => {
            for spelling in target_spellings(index, nesting, t) {
                index.polluted_opaque.insert(spelling);
            }
        }
    }
}

/// Every method name a module reference can inject: its own methods plus
/// those of the modules it includes, chased to a small depth. `None`
/// means "unreadable" — the module does not resolve in this project, or
/// it (or something it includes) is itself an open class, which is the
/// index's own way of saying "a body nobody showed this checker".
fn module_method_names(
    index: &ProjectIndex,
    nesting: &[String],
    path: &str,
) -> Option<Vec<String>> {
    let mut out = Vec::new();
    let mut queue = vec![(path.to_string(), 0usize)];
    let mut seen: Vec<String> = Vec::new();
    while let Some((name, depth)) = queue.pop() {
        if depth > 3 {
            return None;
        }
        if seen.contains(&name) {
            continue;
        }
        seen.push(name.clone());
        let id = index.resolve_const(nesting, &name)?;
        let class = &index.classes[id.0 as usize];
        if class.open {
            return None;
        }
        out.extend(class.methods.keys().cloned());
        out.extend(class.singleton_methods.keys().cloned());
        for inc in class.includes.iter().chain(&class.prepends) {
            queue.push((inc.clone(), depth + 1));
        }
    }
    Some(out)
}

/// Does the client's discovered `sorbet/rbi` genuinely declare `name` as a
/// class/module fragment — not just a scanner false positive from phase
/// 1's cheap line scan (bead ita-vto)? Called from `check.rs`'s
/// `Checker::check_const_ref`, immediately after the project's own index
/// has already failed to resolve `name`, one constant at a time, exactly
/// as `check.rs`'s existing walk discovers each one — never a separate
/// pre-pass over the whole project. Parses, via the same `parse_defs_text`
/// every project file already uses, only the one file phase 1 pointed at
/// for this name; `rbi_file_fragments` memoizes that read+parse process-
/// wide, so the same file is never re-read whether many project files
/// reference the same constant or the same file declares several distinct
/// ones a project references separately.
///
/// A confirmed match resolves the constant (suppresses E0104) but is
/// deliberately never interned into `ProjectIndex`, unlike the curated
/// `gems.rbi` (`merge_declared_fragment`, merged once, unconditionally,
/// into the small shared index every `project_index` call already
/// builds): the reference corpus's `sorbet/rbi` names tens of thousands of
/// distinct constants, and growing the shared, cross-file `ProjectIndex`
/// once per demanded constant means every one of ~2400 project files
/// would clone (or otherwise touch) an index sized by however many other
/// files already triggered a load — exactly the O(files x index size)
/// cost this bead exists to avoid (measured: cloning `ProjectIndex` once
/// per checked file, instead of never, turned a ~0.6s check into 25s+).
/// Every RBI declaration is `open = true` by contract (see
/// `merge_declared_fragment`'s doc comment) — a class with zero methods
/// and no known ancestry — so typing its references as `Ty::Unknown`
/// instead of `Ty::Class`/`Ty::Instance` changes nothing observable:
/// `Unknown` never diagnoses (invariant #1), exactly like an `open`
/// class's `Inconclusive` method lookups never do. A spelled reference
/// that doesn't match a fully-qualified RBI name is simply never found —
/// a false negative, never a false positive. Bead ita-k9j.3: `rbi_map`
/// now maps a name to EVERY declaring file, never just one — a stub
/// reopening and the real declaration are both real at runtime, so the
/// name resolves the moment ANY candidate file's own fragment carries
/// the exact qualified path; short-circuits on the first candidate that
/// resolves, same "per-file question" discipline `resolve_method_node`
/// uses.
pub fn rbi_declares<S: std::hash::BuildHasher>(name: &str, rbi_map: &HashMap<String, Vec<PathBuf>, S>) -> bool {
    let name = name.trim_start_matches("::");
    let Some(paths) = rbi_map_candidates(name, rbi_map) else {
        return false;
    };
    paths
        .iter()
        .any(|path| rbi_file_fragments(path).iter().any(|f| f.path == name))
}

/// Bead ita-y0s: phase 1's line scanner (`rbi.rs::scan_class_name`) trims
/// leading whitespace, so a `class`/`module` header NESTED inside another
/// block in a `.rbi` file (`module Origem; module Coisa; ...; end; end`)
/// registers `rbi_map` under its BARE last simple name ("Coisa"), never
/// the fully qualified path ("`Origem::Coisa`") phase 2's real prism parse
/// (`rbi_file_fragments`) computes for that same fragment via genuine
/// nesting tracking. An exact-key lookup on the full qualified name then
/// dies before phase 2 is ever consulted, even though phase 2 would have
/// answered correctly — measured against a real corpus: `Coisa` reopened
/// under `Origem` in Tapioca's own compact-vs-nested emission is not a
/// hypothetical shape.
///
/// On an exact-key miss, retry under `name`'s bare last `::`-segment —
/// the map's own worst-case key for that name — and return WHATEVER
/// candidates that key carries. This only ever WIDENS which files a
/// caller's own per-file refilter (`f.path == name`, `rbi_declares`'s own
/// exact match; `f.path == owner`, `rbi_qualified_const_declares`'s) gets
/// a chance to inspect; it never widens what counts as a match. A bare
/// name shared by two UNRELATED namespaces in two different `.rbi` files
/// (`Origem::Coisa` and `Baz::Coisa`, both nested, both keyed under
/// bare `"Coisa"`) still degrades to a silent miss for whichever one the
/// query does NOT name: the refilter's exact qualified-path comparison
/// runs against phase 2's real parse of EVERY candidate, so a homonym
/// contributes nothing unless its own real fragment path is the exact
/// name asked for. No fallback at all when `name` already carries no
/// `::` (nothing coarser than the exact key exists to retry).
fn rbi_map_candidates<'a, S: std::hash::BuildHasher>(name: &str, rbi_map: &'a HashMap<String, Vec<PathBuf>, S>) -> Option<&'a Vec<PathBuf>> {
    rbi_map.get(name).or_else(|| {
        let last = name.rsplit("::").next()?;
        if last == name {
            return None;
        }
        rbi_map.get(last)
    })
}

/// Bead ita-47y (RBI-target extension): does the client's Tapioca/Sorbet
/// `sorbet/rbi` declare `qualified` — either as a class/module
/// (`rbi_declares`), or as a plain qualified value-constant write
/// (`Owner::Simple = T.let(...)`, the shape a `gems/*.rbi` writes with
/// the FULL path on the LHS, never nested inside the owner's own
/// class/module body — the real `language_server-protocol` gem's own
/// enum-member constants) recorded in that RBI file's own
/// `FileDefs::qualified_writes`, or a plain bare write NESTED inside the
/// owner's own fragment body (`FileDefs::fragments[_].consts`)?
/// Consulted only from `Checker::check_const_ref`, only after
/// `ProjectIndex::expand_unresolved_alias_target` already handed back an
/// alias's raw target text `resolve_const` could not resolve in-project
/// (the measured ruby-lsp shape: `Interface =
/// LanguageServer::Protocol::Interface`, whose real target lives only in
/// a vendorized `sorbet/rbi/gems/language_server-protocol@*.rbi`, never
/// as a project `ClassId`). Suppression-only, same contract as every
/// other RBI channel (`rbi_declares`'s own doc comment): a hit only
/// silences E0104, never produces a `Ty` (invariant #1) — this project's
/// `ProjectIndex` is never touched.
pub fn rbi_qualified_const_declares<S: std::hash::BuildHasher>(qualified: &str, rbi_map: &HashMap<String, Vec<PathBuf>, S>) -> bool {
    let qualified = qualified.trim_start_matches("::");
    if rbi_declares(qualified, rbi_map) {
        return true;
    }
    let Some((owner, simple)) = qualified.rsplit_once("::") else {
        return false;
    };
    // Bead ita-y0s: same bare-last-segment fallback as `rbi_declares`
    // (see `rbi_map_candidates`'s doc comment) — `owner` itself can be a
    // nested `.rbi` module/class phase 1 only ever keyed under its bare
    // simple name. The per-file refilter right below (`o == owner` /
    // `f.path == owner`, both exact string comparisons against phase 2's
    // real parse) is what keeps a bare-name collision a silent miss
    // rather than a wrong hit — unchanged by this fallback, only fed
    // more candidate files to run against.
    let Some(paths) = rbi_map_candidates(owner, rbi_map) else {
        return false;
    };
    paths.iter().any(|path| {
        let defs = rbi_file_defs(path);
        defs.qualified_writes.iter().any(|(o, s)| o == owner && s == simple)
            || defs
                .fragments
                .iter()
                .any(|f| f.path == owner && f.consts.iter().any(|c| c == simple))
    })
}

/// Bead ita-4wq: guard against a pathological RBI alias cycle
/// (`A = B` / `B = A`, both written INSIDE a vendored `.rbi`) — same
/// role as `ProjectIndex::CONST_ALIAS_CHAIN_CAP` for the project-side
/// chase, sized the same for the same reason (a real alias chain is one
/// or two hops; this only guards against a pathological one).
const RBI_ALIAS_CHAIN_CAP: usize = 32;

/// Bead ita-4wq: is `name` itself the LHS of a qualified const-write
/// INSIDE a vendored RBI whose RHS parses as a literal constant path —
/// the real tapioca/ruby-lsp shape `RubyLsp::Constant =
/// LanguageServer::Protocol::Constant` (a Sorbet RBI re-exporting one
/// gem's namespace under another, written directly in the `.rbi`, never
/// in project code)? Every `.rbi` file is parsed by the exact same
/// `DefWalker` project files are (`rbi_file_defs` -> `parse_defs_text`),
/// so this data was ALREADY harvested into `FileDefs::const_aliases`
/// (bead ita-54k) — no consumer had ever read an RBI file's own
/// `const_aliases` before this bead; every existing alias-chase function
/// (`ProjectIndex::resolve_const_via_alias`,
/// `expand_unresolved_alias_target`) only ever walks the PROJECT's
/// merged `const_aliases`, built exclusively from project files.
/// `owner`'s own candidate files are found the same bare-nesting-aware
/// way `rbi_qualified_const_declares` finds them (`rbi_map_candidates`,
/// bead ita-y0s) — an aliased namespace can itself be nested. `None`
/// when `name` carries no `::` (a bare LHS is out of this bead's
/// measured scope — every real tapioca alias shape found so far writes
/// a fully qualified LHS) or no candidate file's own `const_aliases`
/// names `name` exactly.
fn rbi_alias_lookup<S: std::hash::BuildHasher>(name: &str, rbi_map: &HashMap<String, Vec<PathBuf>, S>) -> Option<String> {
    let (owner, _simple) = name.rsplit_once("::")?;
    let paths = rbi_map_candidates(owner, rbi_map)?;
    paths.iter().find_map(|path| {
        rbi_file_defs(path)
            .const_aliases
            .iter()
            .find(|(lhs, ..)| lhs == name)
            .map(|(_, _, target)| target.clone())
    })
}

/// Bead ita-4wq: fully chase an RBI alias chain starting from an ALREADY
/// CONFIRMED first-hop target (`rbi_alias_lookup`'s return), the same
/// "keep going until the target is no longer itself an alias" shape
/// `ProjectIndex::chase_alias_target_text` uses for the project-side
/// extension — except every hop here is looked up in the RBI's own
/// `const_aliases`, never the project index. Cycle-guarded
/// (`RBI_ALIAS_CHAIN_CAP` iterations, `visited` set): a pathological
/// `A = B` / `B = A` pair written inside one or more `.rbi` files
/// degrades to `None`, never hangs.
fn rbi_alias_leaf<S: std::hash::BuildHasher>(first_target: &str, rbi_map: &HashMap<String, Vec<PathBuf>, S>) -> Option<String> {
    let mut cur = first_target.to_string();
    let mut visited = std::collections::HashSet::new();
    visited.insert(cur.clone());
    for _ in 0..RBI_ALIAS_CHAIN_CAP {
        match rbi_alias_lookup(&cur, rbi_map) {
            None => return Some(cur), // leaf: no further alias hop past this target
            Some(next) => {
                if !visited.insert(next.clone()) {
                    return None; // cycle
                }
                cur = next;
            }
        }
    }
    None // cap exceeded without terminating
}

/// Bead ita-4wq: does SOME prefix of `qualified` (from the shortest
/// two-segment candidate outward to everything but the final segment)
/// name an RBI-declared alias? If so, return the fully-chased target
/// TEXT (`rbi_alias_leaf`) with `qualified`'s remaining `::`-segments
/// re-appended (`join_remaining`, bead ita-47y's own helper) — the
/// expanded candidate a caller retries against the ordinary RBI channels
/// (`rbi_declares`/`rbi_qualified_const_declares`). The SHORTEST hit
/// wins: mirrors left-to-right segment resolution (the earliest point in
/// the path an alias hop could occur). Suppression-only, same contract
/// as every other RBI channel: a hit only silences E0104, never produces
/// a `Ty` (invariant #1) — this project's `ProjectIndex` is never
/// touched, and an alias target that itself resolves nowhere (in the RBI
/// OR the project) simply leaves the reference exactly as unresolved as
/// it was before this bead, never worse.
pub fn rbi_alias_expand<S: std::hash::BuildHasher>(qualified: &str, rbi_map: &HashMap<String, Vec<PathBuf>, S>) -> Option<String> {
    let qualified = qualified.trim_start_matches("::");
    let segments: Vec<&str> = qualified.split("::").collect();
    for i in 1..segments.len() {
        let owner = segments[..i].join("::");
        if let Some(first_target) = rbi_alias_lookup(&owner, rbi_map) {
            let leaf = rbi_alias_leaf(&first_target, rbi_map)?;
            return Some(join_remaining(&leaf, segments[i..].iter().copied()));
        }
    }
    None
}

/// Distinct `.rbi` files `rbi_declares` has actually read and parsed so
/// far in this process — bead ita-vto's acceptance evidence that phase 2
/// stays lazy (nowhere close to every file `RbiProject`'s phase-1 scan
/// found).
pub fn rbi_files_parsed_count() -> usize {
    rbi_cache()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .len()
}

/// Process-wide memo of `path -> parsed fragments`, so the same `.rbi`
/// file is read and parsed at most once across an entire `ita check` run
/// no matter how many project files reference constants it declares. A
/// plain `Mutex`-guarded map rather than a salsa-tracked query:
/// `RbiProject`'s map is wired once per process and a client's `.rbi` tree
/// never changes mid-run (LSP's incremental-edit case is out of this
/// bead's scope — see `AGENTS.md`), so there is nothing here for salsa's
/// revision tracking to buy; a hand-rolled cache is the plain, correct
/// choice.
fn rbi_cache() -> &'static std::sync::Mutex<HashMap<PathBuf, FileDefs>> {
    use std::sync::{LazyLock, Mutex};
    static CACHE: LazyLock<Mutex<HashMap<PathBuf, FileDefs>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));
    &CACHE
}

fn rbi_file_defs(path: &Path) -> FileDefs {
    let mut guard = rbi_cache()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(defs) = guard.get(path) {
        return defs.clone();
    }
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let defs = parse_defs_text(&text);
    guard.insert(path.to_path_buf(), defs.clone());
    defs
}

fn rbi_file_fragments(path: &Path) -> Vec<ClassFragment> {
    rbi_file_defs(path).fragments
}

// -- W3 require/autoload: external (RBI) ancestry for constant lookup ----

/// Every constant simple-name reachable from `start`'s external ancestor
/// closure — the namespaces the project's own index could NOT resolve,
/// walked through the client's Tapioca RBIs exactly the way Ruby walks
/// cref ancestors: superclass edges plus `include`/`prepend` edges (`extend`
/// is deliberately absent — it lands on the singleton, which lexical cref
/// lookup never consults). At each visited namespace the constants are its
/// own `CONST = ...` assignments, its direct child namespaces, and the
/// fully-qualified value assignments Tapioca writes at file toplevel
/// (`A::B::C = T.let(...)`, harvested as `FileDefs::consts`).
///
/// Suppression-only, like every RBI channel (see `rbi_declares`): a hit
/// silences E0104 and never produces a type, so a wrong edge here can
/// only cost a warning, never invent a diagnostic (invariant #1).
/// Memoized per start name — the distinct unresolved-superclass names in
/// a real project are a handful, and each closure is a few nodes over
/// one or two already-cached RBI files.
///
/// ponytail: closure capped at 48 visited namespaces — the deepest real
/// gem ancestry (graphql's Object -> Member -> `GraphQLTypeNames`) is 3;
/// a cap exists only so a pathological cyclic declaration can't spin.
fn rbi_ancestor_closure<S: std::hash::BuildHasher>(
    start: &str,
    rbi_map: &HashMap<String, Vec<PathBuf>, S>,
) -> std::sync::Arc<std::collections::HashSet<String>> {
    let key = start.trim_start_matches("::").to_string();
    if let Some(hit) = closure_memo_get(&key) {
        return hit;
    }
    let out = std::sync::Arc::new(compute_rbi_ancestor_closure(&key, rbi_map));
    closure_memo_put(key, out.clone());
    out
}

fn closure_memo(
) -> &'static std::sync::Mutex<HashMap<String, std::sync::Arc<std::collections::HashSet<String>>>> {
    use std::sync::{Arc, LazyLock, Mutex};
    static MEMO: LazyLock<Mutex<HashMap<String, Arc<std::collections::HashSet<String>>>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));
    &MEMO
}

fn closure_memo_get(key: &str) -> Option<std::sync::Arc<std::collections::HashSet<String>>> {
    closure_memo()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(key)
        .cloned()
}

fn closure_memo_put(key: String, val: std::sync::Arc<std::collections::HashSet<String>>) {
    closure_memo()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(key, val);
}

/// The BFS itself (W3): see `rbi_ancestor_closure`'s doc comment for the
/// contract and the suppression-only safety argument. Bead ita-k9j.3: a
/// visited node's constants/edges are the UNION over every file that
/// declares it, never just one — a stub reopening still contributes
/// whatever ancestry it names.
fn compute_rbi_ancestor_closure<S: std::hash::BuildHasher>(
    start: &str,
    rbi_map: &HashMap<String, Vec<PathBuf>, S>,
) -> std::collections::HashSet<String> {
    let mut consts = std::collections::HashSet::new();
    let mut visited = std::collections::HashSet::new();
    let mut work: Vec<String> = vec![start.to_string()];
    // ponytail: closure capped at 48 visited namespaces — the deepest real
    // gem ancestry (graphql's Object -> Member -> GraphQLTypeNames) is 3;
    // the cap only keeps a pathological cyclic declaration from spinning.
    while let Some(node) = work.pop() {
        if visited.contains(&node) || visited.len() >= 48 {
            continue;
        }
        visited.insert(node.clone());
        let Some(files) = rbi_map.get(&node) else {
            continue;
        };
        for file in files {
            let defs = rbi_file_defs(file);
            harvest_rbi_consts(&defs, &node, &mut consts);
            queue_rbi_edges(&defs, &node, &mut work);
        }
    }
    consts
}

/// Constants declared ON `node`: its own `CONST = ...` assignments, its
/// direct child namespaces (`class Node::Child` headers — one level
/// only, matching Ruby's cref lookup), and Tapioca's toplevel
/// fully-qualified value writes (`A::B::C = T.let(...)`).
fn harvest_rbi_consts(defs: &FileDefs, node: &str, consts: &mut std::collections::HashSet<String>) {
    let prefix = format!("{node}::");
    for name in defs
        .fragments
        .iter()
        .filter(|f| f.path == node)
        .flat_map(|f| f.consts.iter().cloned())
    {
        consts.insert(name);
    }
    let direct: Vec<String> = defs
        .fragments
        .iter()
        .filter_map(|f| direct_child(&f.path, &prefix))
        .chain(
            defs.consts
                .iter()
                .filter_map(|(q, ..)| direct_child(q, &prefix)),
        )
        .collect();
    consts.extend(direct);
}

/// `Some(child)` when `path` is exactly `prefix + child` with no deeper
/// `::` — a one-level-cref name on the prefix's owner.
fn direct_child(path: &str, prefix: &str) -> Option<String> {
    let rest = path.strip_prefix(prefix)?;
    (!rest.contains("::")).then(|| rest.to_string())
}

/// Ancestry edges out of `node`'s own fragment: the written superclass
/// and every `include`/`prepend` (`extend` never reaches cref lookup).
fn queue_rbi_edges(defs: &FileDefs, node: &str, work: &mut Vec<String>) {
    let Some(frag) = defs.fragments.iter().find(|f| f.path == node) else {
        return;
    };
    if let Some(sc) = &frag.superclass {
        work.push(sc.trim_start_matches("::").to_string());
    }
    for edge in frag.includes.iter().chain(frag.prepends.iter()) {
        work.push(edge.trim_start_matches("::").to_string());
    }
}

/// W3: does `name`, referenced from lexical `scope`, resolve through an
/// EXTERNAL ancestor — a superclass or mixin the project's own index
/// could not resolve, but the client's Tapioca RBIs declare (with its
/// constants and its own ancestry)? This is the `ID`/`Boolean`/`Int`
/// shape: graphql-ruby apps reference the `GraphQLTypeNames` mixin's
/// constants from inside `class Types::Query < GraphQL::Schema::Object`,
/// and the constant is real exactly because the gem's ancestry carries
/// it — invisible to a project-only index, plain as day in the RBI.
///
/// Consulted only from `check.rs::check_const_ref`, only after the
/// project's own `const_exists` failed and the flat `rbi_declares` (name
/// as written) missed. Like both, a hit only suppresses E0104 — the
/// reference still types as `Ty::Unknown`, so no new E0101/E0102/E0103
/// can appear (invariant #1).
pub fn rbi_ancestor_declares<S: std::hash::BuildHasher>(
    index: &ProjectIndex,
    nesting: &[String],
    name: &str,
    rbi_map: &HashMap<String, Vec<PathBuf>, S>,
) -> bool {
    let (starts, simple) = index.external_lookup_starts(nesting, name);
    if starts.is_empty() {
        return false;
    }
    starts
        .iter()
        .any(|s| rbi_ancestor_closure(s, rbi_map).contains(simple))
}

/// Instance method names, then SINGLETON method names, that a start
/// name's RBI ancestry declares — mapped to the RAW `.returns(...)`
/// source text of the method's `.rbi` sig (bead ita-uh1's
/// `MethodDef::sorbet_ret`), `None` for a method with no sig. Text, not
/// `Ty` (bead ita-tjr): the memo below is keyed on the start name alone
/// and shared by every call site, so it cannot depend on any one call's
/// `ProjectIndex` — resolving a PROJECT class name inside the sig
/// (`sig { returns(::Package) }` in a DSL RBI) needs that index, so the
/// text-to-`Ty` conversion (`sorbet_sig::resolve_ret_ty`) happens at the
/// lookup call site instead, where the index is in hand. A hit is still
/// a hit whether or not the text resolves to something other than
/// `Ty::Unknown` (see `rbi_method_lookup`/`dsl_method_lookup`). The
/// order is load-bearing: `extend` and `mixes_in_class_methods` move a
/// module's *instance* methods into the singleton slot (that is how
/// `Model.where` exists), so swapping the two silently turns every
/// class-method hit into an instance-method hit. `Arc` because the memo
/// hands the same maps to many call sites.
type RbiMethodSets = std::sync::Arc<(HashMap<String, Option<String>>, HashMap<String, Option<String>>)>;

/// One BFS work item: the RBI name to resolve next, which method-
/// dispatch track it travels on, and — when it names an edge queued
/// FROM another fragment (`queue_method_edges`) — that parent
/// fragment's own file and fully-qualified path (bead ita-tjr). The
/// parent context is what lets `resolve_method_node` try the SAME file
/// before ever touching the cross-file `rbi_map`; the very first work
/// item (the walk's `start`) carries `parent: None` and always resolves
/// through `rbi_map`, exactly as before this field existed.
struct MethodWorkItem {
    node: String,
    on_singleton_track: bool,
    parent: Option<(PathBuf, String)>,
}

/// Method-name BFS over the RBI world, keeping instance and singleton
/// methods in separate maps — but NOT a plain mirror of
/// `rbi_ancestor_closure`'s constant walk, because method dispatch has an
/// edge `queue_rbi_edges` doesn't model: `extend`. Four rules, measured
/// against a real `sorbet/rbi/gems/activerecord@*.rbi` (lead review,
/// bead ita-xze, 2026-08-21):
///
/// 1. `include`/`prepend` of `X` walked from an INSTANCE-track node keeps
///    walking `X` on the INSTANCE track — `X`'s own `methods` land in
///    the instance map.
/// 2. `extend X` from an INSTANCE-track node switches to a
///    SINGLETON-track walk of `X` — `X`'s `methods` (not
///    `singleton_methods`) land in the SINGLETON map. This is why
///    `Model.where` exists at all: `where` is an INSTANCE method of
///    `ActiveRecord::Querying`, and `ActiveRecord::Base` does
///    `extend ::ActiveRecord::Querying`.
/// 3. `mixes_in_class_methods X` (Sorbet's literal rendering of the
///    `ActiveSupport::Concern` `ClassMethods` idiom) is the same
///    singleton-track switch as rule 2 — see `ClassFragment`'s doc
///    comment on the field.
/// 4. `def self.x` / `class << self` on ANY visited node — instance or
///    singleton track — lands in the SINGLETON map unconditionally
///    (`frag.singleton_methods`, harvested at every node regardless of
///    how it was reached).
///
/// Once on the singleton track, only rules 1/4 continue the walk — a
/// nested `extend`/`mixes_in_class_methods` inside the extended module's
/// OWN body affects that module's own singleton, never the original
/// start's, so it is not followed a second time (matches real Ruby:
/// `extend` pulls in the target's ANCESTOR chain, not the target's own
/// singleton).
///
/// A fifth rule, added for the Tapioca DSL RBIs a client's OWN app
/// models carry (bead ita-tjr, not gem RBIs): `resolve_method_node`
/// resolves every edge against the PARENT fragment's own file before
/// ever touching the global `rbi_map` — a DSL RBI reopens a project
/// class and, in the SAME file, nests the real module the class
/// unqualifiedly `include`s (`class Package; include
/// GeneratedAssociationMethods; end` with `module
/// Package::GeneratedAssociationMethods` nested later in that same
/// file). The global map cannot be trusted for these names: phase 1
/// (`rbi.rs::build_rbi_index`) is a naive per-line scan that records
/// every `module GeneratedAssociationMethods` header it sees — nested or
/// not — as a TOP-LEVEL name, first file wins; a real project's
/// `sorbet/rbi/dsl/` carries that identical generated module name in
/// every model's own DSL file (~1587 at the measured reference corpus),
/// so resolving through the global map would silently attribute one
/// arbitrary model's accessors to every other model.
///
/// Name collision within one walk (bead ita-uh1): the FIRST visit wins
/// — `compute_rbi_method_closure` never overwrites an already-recorded
/// name — because the walk already runs in MRO order; if two ancestors
/// disagree on a method's return type, the nearer one is the one that
/// actually answers at runtime.
///
/// Memoized per `start`, exactly like `rbi_ancestor_closure`: the bead
/// this feeds (ita-xze, extended by ita-uh1, ita-tjr) is consulted once
/// per `Inconclusive` call site, and the distinct start names in a real
/// project are a handful (external ancestors) to one-per-model (DSL
/// starts) shared by many call sites — re-walking the same RBI chain per
/// call site would be the exact O(files-times-call-sites) reparse cost
/// `rbi_declares`'s doc comment measures at 25s+ for interning alone;
/// this reuses `rbi_file_defs`'s own process-wide read+parse cache and
/// adds a memo on the BFS result itself on top.
///
/// The memo is keyed on the START NAME alone, not on which `rbi_map`
/// supplied it — correct for `ita check` and the LSP, where one process
/// serves one project, and the reason a single run never re-walks
/// `ActiveRecord::Base` for each of thousands of models. It does mean two
/// different RBI trees in ONE process serve each other's cached method
/// set: harmless in production, but it makes tests fail by run order, so
/// every test in `rbi_methods.rs`/`dsl_rbi.rs` uses a distinct start
/// name.
/// ponytail: key on `(start, rbi tree root)` if a caller ever needs two
/// projects live in one process.
fn rbi_method_closure<S: std::hash::BuildHasher>(start: &str, rbi_map: &HashMap<String, Vec<PathBuf>, S>) -> RbiMethodSets {
    let key = start.trim_start_matches("::").to_string();
    if let Some(hit) = method_closure_memo_get(&key) {
        return hit;
    }
    let out = std::sync::Arc::new(compute_rbi_method_closure(&key, rbi_map));
    method_closure_memo_put(key, out.clone());
    out
}

/// Bead ita-k9j: read-only accessor over `rbi_method_closure`'s BFS
/// result — instance names, then singleton names, sorted — so
/// `scripts/gen-activerecord-inventory.rs` can harvest a start name's
/// real declared API through the EXACT SAME four edge rules
/// `rbi_escalate`/`lookup_method_rbi` consult at check time (see
/// `rbi_method_closure`'s doc comment for the four rules), instead of a
/// second, independently invented RBI walk. Names only: the mapped
/// `.rbi` sig text this bead's callers don't need is dropped here.
/// Never called from the check path itself.
pub fn rbi_method_closure_names<S: std::hash::BuildHasher>(
    start: &str,
    rbi_map: &HashMap<String, Vec<PathBuf>, S>,
) -> (Vec<String>, Vec<String>) {
    let (instance, singleton) = &*rbi_method_closure(start, rbi_map);
    let mut inst: Vec<String> = instance.keys().cloned().collect();
    let mut sing: Vec<String> = singleton.keys().cloned().collect();
    inst.sort();
    sing.sort();
    (inst, sing)
}

fn method_closure_memo() -> &'static std::sync::Mutex<HashMap<String, RbiMethodSets>> {
    use std::sync::{LazyLock, Mutex};
    static MEMO: LazyLock<Mutex<HashMap<String, RbiMethodSets>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));
    &MEMO
}

fn method_closure_memo_get(key: &str) -> Option<RbiMethodSets> {
    method_closure_memo()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(key)
        .cloned()
}

fn method_closure_memo_put(key: String, val: RbiMethodSets) {
    method_closure_memo()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(key, val);
}

/// Cap on distinct `(node, track)` pairs visited by `compute_rbi_method_closure`.
/// A pure anti-cycle guard, NOT a depth budget: `rbi_ancestor_closure`'s
/// 48 was sized for the deepest real CONSTANT chain (3 hops); method
/// dispatch through a framework base class is a different shape entirely
/// — `sorbet/rbi/gems/activerecord@*.rbi`'s `ActiveRecord::Base` alone
/// carries 123 direct `include`/`extend` edges (lead review, bead
/// ita-xze, 2026-08-21), several of which (e.g.
/// `ActiveRecord::AttributeMethods`) recurse into a dozen more. A cap
/// tuned to that shape has to clear low thousands of nodes; 48 would
/// truncate the walk long before it ever reached the methods that make
/// this bead worth doing, turning a measured win into a false near-zero.
/// Do not shrink this back toward `rbi_ancestor_closure`'s constant.
const RBI_METHOD_CLOSURE_CAP: usize = 4096;

/// Resolve one BFS work item to the fully-qualified fragment path it
/// actually names, PLUS every file that contributes to it (bead
/// ita-tjr, extended to a union by bead ita-k9j.3). Three tries, in
/// order, and the order is load-bearing (see `rbi_method_closure`'s doc
/// comment for the measured collision this exists to avoid):
///
/// (a) the edge text itself, as an exact fragment path already present
///     in the PARENT's own file (an ordinary un-nested reopening the
///     same file also happens to declare under this literal name);
/// (b) `<parent path>::<edge text>` in the parent's own file — the
///     nested Tapioca DSL shape (`Package::GeneratedAssociationMethods`
///     nested inside `class Package`, `include`d unqualified);
/// (c) the global `rbi_map`, exactly the only path that existed before
///     this bead — correct and sufficient for ordinary gem RBIs, one
///     class per file, no nesting.
///
/// (a) and (b) are PER-FILE questions answered against the ONE parent
/// file this work item actually carries — a stub reopening never
/// changes that: each candidate file a walk visits pushes its OWN
/// edges with itself as parent (see `compute_rbi_method_closure`), so
/// a downstream edge's (a)/(b) trial is always scoped to the single
/// file it came from, never blended across candidates. Only (c), the
/// last-resort global map, can name several files at once (bead
/// ita-k9j.3: a stub reopening and the real declaration are both real
/// at runtime) — every one of them is returned, and
/// `compute_rbi_method_closure` walks each in turn. This stays safe
/// against the `dsl/` bare-name collision `rbi.rs`'s module doc warns
/// about: whichever files (c) returns are still individually refiltered
/// by `frag.path == resolved` before contributing a single method, so a
/// file that only nests the name (never declares it at true top level)
/// contributes nothing — a collision still degrades to a miss, never a
/// wrong type.
///
/// The very first work item (a walk's `start`) carries no parent, so it
/// always falls straight through to (c).
fn resolve_method_node<S: std::hash::BuildHasher>(
    node: &str,
    parent: Option<&(PathBuf, String)>,
    rbi_map: &HashMap<String, Vec<PathBuf>, S>,
) -> Option<(String, Vec<PathBuf>)> {
    if let Some((parent_file, parent_path)) = parent {
        let defs = rbi_file_defs(parent_file);
        if defs.fragments.iter().any(|f| f.path == node) {
            return Some((node.to_string(), vec![parent_file.clone()]));
        }
        let qualified = format!("{parent_path}::{node}");
        if defs.fragments.iter().any(|f| f.path == qualified) {
            return Some((qualified, vec![parent_file.clone()]));
        }
    }
    rbi_map.get(node).map(|files| (node.to_string(), files.clone()))
}

/// The BFS itself: see `rbi_method_closure`'s doc comment for the
/// four-plus-one-rule contract and the first-visit-wins collision rule.
/// `filter` (not `find`) over fragments matching the resolved path, like
/// `harvest_rbi_consts`: a single `.rbi` file can reopen the same path
/// more than once (Tapioca emits a fresh `sig`-guarded block per overload
/// set), and a `find` would silently drop every method after the first
/// block. `visited` keys on `(resolved path, track)`, not the raw edge
/// text: two DIFFERENT edges spelled the same way (`include
/// GeneratedAssociationMethods` in two unrelated DSL files) resolve to
/// two DIFFERENT qualified paths via `resolve_method_node`, and must
/// never be treated as the same node. `entry(...).or_insert_with(...)`
/// everywhere a name is recorded, never `insert`/`extend`: that is what
/// makes the first visit win.
///
/// Bead ita-k9j.3: `resolved` can now name SEVERAL files at once (a stub
/// reopening plus the real declaration) — every one of them is walked
/// under the SAME `visited` entry, contributing its own fragments'
/// methods (union of names, first-file-in-`files`-order wins a same-name
/// conflict — deterministic, since `files` is built from
/// `discover_rbi_files`'s sorted output) and queuing its own edges with
/// ITSELF as the parent file, so a stub's edges never get resolved
/// against the full declaration's file or vice versa.
fn compute_rbi_method_closure<S: std::hash::BuildHasher>(
    start: &str,
    rbi_map: &HashMap<String, Vec<PathBuf>, S>,
) -> (HashMap<String, Option<String>>, HashMap<String, Option<String>>) {
    let mut instance: HashMap<String, Option<String>> = HashMap::new();
    let mut singleton: HashMap<String, Option<String>> = HashMap::new();
    let mut visited: std::collections::HashSet<(String, bool)> = std::collections::HashSet::new();
    let mut work: Vec<MethodWorkItem> = vec![MethodWorkItem {
        node: start.to_string(),
        on_singleton_track: false,
        parent: None,
    }];
    while let Some(item) = work.pop() {
        let Some((resolved, files)) =
            resolve_method_node(&item.node, item.parent.as_ref(), rbi_map)
        else {
            continue;
        };
        let key = (resolved.clone(), item.on_singleton_track);
        if visited.contains(&key) || visited.len() >= RBI_METHOD_CLOSURE_CAP {
            continue;
        }
        visited.insert(key);
        for file in &files {
            let defs = rbi_file_defs(file);
            for frag in defs.fragments.iter().filter(|f| f.path == resolved) {
                harvest_frag_methods(frag, item.on_singleton_track, &mut instance, &mut singleton);
                queue_method_edges(frag, item.on_singleton_track, file, &mut work);
            }
        }
    }
    (instance, singleton)
}


/// Rules 1/2/4: a node's own `def self.x` always lands in `singleton`,
/// while its INSTANCE methods land in whichever map the track that
/// reached it selects. First visit wins (`or_insert_with`) because the
/// walk already runs in MRO order — the nearest ancestor is the one that
/// really answers at runtime. Values are the raw `.returns(...)` sig
/// text (`MethodDef::sorbet_ret`), `None` for an unsigned method — see
/// `RbiMethodSets`'s doc comment for why the `Ty` conversion is deferred
/// to the lookup call site.
fn harvest_frag_methods(
    frag: &ClassFragment,
    on_singleton_track: bool,
    instance: &mut HashMap<String, Option<String>>,
    singleton: &mut HashMap<String, Option<String>>,
) {
    for m in &frag.singleton_methods {
        singleton.entry(m.name.clone()).or_insert_with(|| m.sorbet_ret.clone());
    }
    let target = if on_singleton_track { singleton } else { instance };
    for m in &frag.methods {
        target.entry(m.name.clone()).or_insert_with(|| m.sorbet_ret.clone());
    }
}

/// Rules 1/2/3: `superclass`/`include`/`prepend` keep the current track;
/// `extend`/`mixes_in_class_methods` switch TO the singleton track, and
/// only from the instance track — see `rbi_method_closure`'s doc comment
/// for why a second switch never happens. Every pushed item carries
/// `frag`'s own file and path as its parent context (bead ita-tjr), so
/// `resolve_method_node` tries this SAME file first when the edge is
/// popped.
fn queue_method_edges(
    frag: &ClassFragment,
    on_singleton_track: bool,
    file: &Path,
    work: &mut Vec<MethodWorkItem>,
) {
    let strip = |n: &String| n.trim_start_matches("::").to_string();
    let parent = Some((file.to_path_buf(), frag.path.clone()));
    for edge in frag.superclass.iter().chain(frag.includes.iter()).chain(frag.prepends.iter()) {
        work.push(MethodWorkItem {
            node: strip(edge),
            on_singleton_track,
            parent: parent.clone(),
        });
    }
    if !on_singleton_track {
        for edge in frag.extends.iter().chain(frag.mixes_in_class_methods.iter()) {
            work.push(MethodWorkItem {
                node: strip(edge),
                on_singleton_track: true,
                parent: parent.clone(),
            });
        }
    }
}

/// Shared walk for `rbi_method_lookup`/`dsl_method_lookup` (bead
/// ita-tjr): try each start name's RBI method closure in order, and on
/// the first name that DECLARES `method` (instance or singleton side per
/// `singleton`), convert its raw sig text to `Ty` via
/// `sorbet_sig::resolve_ret_ty` — the conversion needs `index` (to
/// resolve a project class name inside the sig), which `rbi_method_closure`'s
/// memo deliberately does not carry (see `RbiMethodSets`'s doc comment).
fn method_lookup_via_starts<S: std::hash::BuildHasher>(
    starts: &[String],
    method: &str,
    singleton: bool,
    rbi_map: &HashMap<String, Vec<PathBuf>, S>,
    index: &ProjectIndex,
) -> Option<Ty> {
    starts.iter().find_map(|start| {
        let (instance, singleton_methods) = &*rbi_method_closure(start, rbi_map);
        let map = if singleton { singleton_methods } else { instance };
        map.get(method)
            .map(|raw| crate::sorbet_sig::resolve_ret_ty(raw.as_deref(), index))
    })
}

/// Does some EXTERNAL ancestor of `id` — a name the project's own index
/// could not resolve, or a `declarations/gems.rbi` force-open entry
/// (`OpenReason::DeclaredExternal`) — declare `method` in the client's
/// Tapioca RBIs (bead ita-xze), and if so, what return `Ty` does its sig
/// map to (bead ita-uh1)? Called only after `lookup_method`/
/// `lookup_singleton` already returned `Inconclusive`: the ancestry-open
/// census this bead's parent measurement ran found 61-86% of that bucket
/// blocked by exactly this — an external ancestor a project-only index
/// can never see into.
///
/// `None` = miss: no external ancestor declares `method` anywhere in its
/// RBI closure. Changes NOTHING (the call site stays `Inconclusive`,
/// typed `Ty::Unknown`, exactly today's behavior).
///
/// `Some(ty)` = hit, and `ty` is very often `Ty::Unknown` itself (no sig,
/// or a sig shape this bead doesn't map) — that is still a hit: the call
/// site is conclusive either way, only the TYPE differs. Aditive-only by
/// construction, which is what keeps a false positive impossible here:
/// arity is never checked on a hit (no `MethodSig`/`RbsSig` rides along,
/// deliberately — see the batch contract), and `sorbet_sig::resolve_ret_ty`
/// only ever produces a core `Ty`, a collection of one, or a project
/// `Ty::Instance`/`Ty::Class` it can prove via `index` — so a wrong `ty`
/// can only ever be a wrong core-type guess or a wrong resolution of a
/// spelled-out class name, feeding invariant #1's existing
/// "Unknown/core receiver never conclusively diagnoses without
/// `ClosedWorld`" guarantee, never invent a project-class fact out of
/// nothing. No `NotFound` is ever produced from an RBI signal, on
/// purpose: Tapioca coverage is never asserted complete, so "the RBI
/// doesn't declare it" proves nothing about the real gem's surface.
pub fn rbi_method_lookup<S: std::hash::BuildHasher>(
    index: &ProjectIndex,
    id: ClassId,
    method: &str,
    singleton: bool,
    rbi_map: &HashMap<String, Vec<PathBuf>, S>,
) -> Option<Ty> {
    method_lookup_via_starts(&index.external_ancestor_starts(id), method, singleton, rbi_map, index)
}

/// Does the client's Tapioca DSL RBI for `id` ITSELF — or for a PROJECT
/// ancestor of `id` (one that resolves in the project's own index, e.g.
/// a shared concern module) — declare `method` (bead ita-tjr)? The DSL
/// RBIs Tapioca writes under `sorbet/rbi/dsl/` reopen the app's OWN
/// model classes (`class Package; include GeneratedAssociationMethods;
/// ...; end`), never an external gem namespace, so the start names here
/// are PROJECT paths (`id`'s own `path`, then every project ancestor's
/// `path` in MRO order) — the opposite population from
/// `rbi_method_lookup`'s external-ancestor walk, and never combined with
/// it: an ancestor this project could not resolve at all has no `path`
/// to look a DSL file up by, and a `DeclaredExternal` ancestor is
/// external by construction. `None` = miss, changes nothing (aditive-
/// only, same contract as `rbi_method_lookup` — see that function's doc
/// comment for the full false-positive argument, which applies
/// unchanged here).
pub fn dsl_method_lookup<S: std::hash::BuildHasher>(
    index: &ProjectIndex,
    id: ClassId,
    method: &str,
    singleton: bool,
    rbi_map: &HashMap<String, Vec<PathBuf>, S>,
) -> Option<Ty> {
    method_lookup_via_starts(&index.project_ancestor_starts(id), method, singleton, rbi_map, index)
}

/// Instance method names each fragment set declares per core namespace —
/// the pure half of the Tapioca closed-world lookup (w12 closure). Only
/// `fragments`' own `methods` (instance methods): `def self.x` in a gem's
/// `class String` reopening lands on String-the-class-object, which no
/// `Ty::Str` receiver ever dispatches through. Names only, no arity: the
/// conclusive lookup asks exactly "does this method exist", and a
/// declared method exists whatever its signature.
pub fn core_methods_of(
    fragments: &[ClassFragment],
) -> HashMap<String, std::collections::HashSet<String>> {
    let mut out: HashMap<String, std::collections::HashSet<String>> = HashMap::new();
    for frag in fragments {
        if crate::core::is_core_namespace(&frag.path) {
            out.entry(frag.path.clone())
                .or_default()
                .extend(frag.methods.iter().map(|m| m.name.clone()));
        }
    }
    out
}

/// `core namespace -> instance method names declared across every `.rbi`
/// reopening of it` (w12 closure, Tapioca closed world). Memoized by
/// salsa on the `RbiCoreReopenings` input (wired once per process), with
/// the per-file read+parse itself memoized by `rbi_cache` — so however
/// many call sites consult it, the reopenings' RBIs are parsed exactly
/// once per run. Never called outside the closed-world conclusive path,
/// so projects without full Tapioca coverage never pay the parse.
#[salsa::tracked]
pub fn rbi_core_methods(
    db: &dyn salsa::Database,
) -> HashMap<String, std::collections::HashSet<String>> {
    let mut out: HashMap<String, std::collections::HashSet<String>> = HashMap::new();
    let Some(reopenings) = crate::RbiCoreReopenings::try_get(db) else {
        return out;
    };
    for files in reopenings.map(db).values() {
        for file in files {
            for (ns, methods) in core_methods_of(&rbi_file_fragments(file)) {
                out.entry(ns).or_default().extend(methods);
            }
        }
    }
    out
}

fn method_sig(md: &MethodDef, file: SourceFile) -> MethodSig {
    MethodSig {
        required: md.required,
        optional: md.optional,
        rest: md.rest,
        keywords: md.keywords.clone(),
        kwrest: md.kwrest,
        sig: md.sig.clone(),
        sorbet_ret: md.sorbet_ret.clone(),
        arity_unknown: md.arity_unknown,
        abstract_stub: md.abstract_stub,
        file,
        def_span: md.def_span,
        name_span: md.name_span,
        schema_col_type: None,
    }
}

/// Result of a method lookup on a project class.
#[derive(Debug)]
pub enum MethodLookup<'a> {
    Found(&'a MethodSig, ClassId),
    /// Ancestry fully resolved and closed; the method does not exist.
    NotFound,
    /// Open class or unresolved ancestry: everything is Unknown.
    Inconclusive,
}

impl ProjectIndex {
    fn intern(&mut self, path: &str) -> ClassId {
        if let Some(&id) = self.by_path.get(path) {
            return id;
        }
        #[expect(
            clippy::cast_possible_truncation,
            reason = "class-table index bounded by `self.classes.len()`, far below u32::MAX for any real Ruby project"
        )]
        let id = ClassId(self.classes.len() as u32);
        self.classes.push(ClassDef {
            path: path.to_string(),
            nesting: Vec::new(),
            is_module: false,
            open: false,
            open_reason: None,
            superclass: None,
            includes: Vec::new(),
            prepends: Vec::new(),
            extends: Vec::new(),
            methods: FxHashMap::default(),
            singleton_methods: FxHashMap::default(),
            consts: Vec::new(),
            hook_instance_installs: Vec::new(),
            hook_singleton_installs: Vec::new(),
            hook_installs_opaque: false,
            table_name: None,
        });
        self.by_path.insert(path.to_string(), id);
        id
    }

    pub fn class(&self, id: ClassId) -> &ClassDef {
        &self.classes[id.0 as usize]
    }

    /// Resolve `name` (a constant path as written) from the REAL lexical
    /// `Module.nesting` chain in effect at the reference site (`nesting`,
    /// outermost first — see `ClassFragment::nesting`'s doc comment).
    /// Ruby lexical lookup approximation: innermost nesting level
    /// outward, then toplevel.
    ///
    /// Bead ita-519: this used to take a flat `scope: &str` and derive
    /// "outer levels" by truncating it at every `::`, which is wrong for
    /// a compact-syntax class/module (`class A::B::C` has
    /// `Module.nesting == [A::B::C]`, ONE level — `A::B` and `A` are NOT
    /// separately searchable lexical scopes there, even though they are
    /// real classes elsewhere in the project). That silently resolved a
    /// bare name against a SIBLING class instead of a top-level one (6
    /// confirmed FPs in rails/rails, e.g. `TestServer` inside a compact
    /// `class ActionCable::Connection::AuthorizationTest` resolving to
    /// the sibling `ActionCable::Connection::TestServer` instead of the
    /// real top-level `::TestServer`). `nesting` now carries the REAL
    /// chain, built by the walker as it actually recurses (`DefWalker`/
    /// `check.rs`'s `Checker`, both push exactly one level per
    /// `class`/`module` keyword, never per `::`-segment), so no
    /// per-call truncation guesswork is needed here at all.
    pub fn resolve_const(&self, nesting: &[String], name: &str) -> Option<ClassId> {
        if let Some(rest) = name.strip_prefix("::") {
            return self.by_path.get(rest).copied();
        }
        let single_segment = !name.contains("::");
        for level in nesting.iter().rev() {
            let candidate = format!("{level}::{name}");
            if let Some(&id) = self.by_path.get(&candidate) {
                return Some(id);
            }
            // A plain `NAME = expr` in this lexical scope shadows any outer
            // class of the same name (`Result = Struct.new` vs a real
            // `Deployment::Result` class): resolution fails to Unknown.
            if single_segment {
                if let Some(&sid) = self.by_path.get(level) {
                    if self.class(sid).consts.iter().any(|c| c == name) {
                        return None;
                    }
                }
            }
        }
        self.by_path.get(name).copied()
    }

    /// Resolve the constant written as a class's superclass (`class X <
    /// NAME`) — bead ita-t6m, the Pundit `class Scope < Scope` shape
    /// (`app/policies/*_policy.rb < ApplicationPolicy`, re-declaring
    /// `Scope` in every policy that subclasses it). Ruby evaluates the
    /// superclass expression BEFORE `X` exists, so `resolve_const`'s
    /// plain lexical walk is wrong here in two ways: (1) a candidate that
    /// happens to resolve back to `id` itself — the class currently being
    /// defined — is not a real answer, it's `nesting`'s own path
    /// reconstructed from an outer level (`DataImportPolicy` + `::Scope`
    /// == `DataImportPolicy::Scope`, which IS `id`); (2) once lexical
    /// nesting is exhausted, Ruby's real algorithm falls through to the
    /// ancestors of the innermost ENCLOSING class, not straight to a
    /// top-level constant — `ApplicationPolicy::Scope` is reached because
    /// `DataImportPolicy < ApplicationPolicy`, not because it's lexically
    /// nested inside `DataImportPolicy`. Order matters: a nearer ancestor
    /// must win over an unrelated top-level same-named class (mutant
    /// table entry b in the ita-t6m report; see
    /// `testdata/pundit_scope/outer_scope_vs_ancestor_scope.rb`).
    /// Provably unresolved either way stays `None` — every caller here
    /// already treats that as leaving the ancestry open (invariant #1:
    /// never a false E0101 from a superclass this index can't pin down).
    fn resolve_superclass_const(&self, id: ClassId, nesting: &[String], name: &str) -> Option<ClassId> {
        self.resolve_superclass_lexical(id, nesting, name)
            .or_else(|| self.resolve_superclass_fallback(id, nesting, name))
    }

    /// Lexical half of `resolve_superclass_const`: same per-level walk as
    /// `resolve_const`, except a candidate resolving to `id` itself is
    /// skipped (keep walking outward) rather than accepted, and — unlike
    /// `resolve_const` — no top-level fallback: that's `resolve_superclass_
    /// fallback`'s job, run only after the real ancestor chain has had its
    /// turn (a nearer ancestor must outrank an unrelated top-level name).
    fn resolve_superclass_lexical(&self, id: ClassId, nesting: &[String], name: &str) -> Option<ClassId> {
        if let Some(rest) = name.strip_prefix("::") {
            return self.by_path.get(rest).copied().filter(|&r| r != id);
        }
        let single_segment = !name.contains("::");
        for level in nesting.iter().rev() {
            let candidate = format!("{level}::{name}");
            match self.by_path.get(&candidate) {
                Some(&found) if found != id => return Some(found),
                Some(_) => {} // resolves to `id` itself: keep walking outward
                None if single_segment => {
                    if let Some(&sid) = self.by_path.get(level) {
                        if self.class(sid).consts.iter().any(|c| c == name) {
                            return None;
                        }
                    }
                }
                None => {}
            }
        }
        None
    }

    /// Ancestor-then-top-level half of `resolve_superclass_const`: once
    /// lexical nesting is exhausted, walk the ancestors of the innermost
    /// ENCLOSING class (`nesting`'s second-to-last entry — the last entry
    /// is always `id` itself, see `resolve_const`'s doc comment) nearest
    /// first, checking each ancestor's own nested-class namespace for
    /// `name`. Only once no real ancestor answers does a bare top-level
    /// constant get a look — mirroring `Object` being the final, least
    /// specific link in every ancestor chain in real Ruby.
    fn resolve_superclass_fallback(&self, id: ClassId, nesting: &[String], name: &str) -> Option<ClassId> {
        if let Some(found) = self.resolve_superclass_ancestor(id, nesting, name) {
            return Some(found);
        }
        self.by_path.get(name).copied().filter(|&r| r != id)
    }

    fn resolve_superclass_ancestor(&self, id: ClassId, nesting: &[String], name: &str) -> Option<ClassId> {
        let enclosing = nesting.get(nesting.len().checked_sub(2)?)?;
        let enclosing_id = *self.by_path.get(enclosing)?;
        let (chain, _complete) = self.ancestors(enclosing_id);
        chain.into_iter().find_map(|a| {
            let candidate = format!("{}::{name}", self.class(a).path);
            self.by_path.get(&candidate).copied().filter(|&r| r != id)
        })
    }

    /// MRO linearization: prepends (reversed) -> self -> includes (reversed)
    /// -> superclass chain. Cycle-safe via visited set. `complete` is false
    /// when any named superclass/mixin failed to resolve in the index —
    /// callers must then treat lookups as Inconclusive.
    pub fn ancestors(&self, id: ClassId) -> (Vec<ClassId>, bool) {
        let mut out = Vec::new();
        let mut complete = true;
        let mut visited = std::collections::HashSet::new();
        self.linearize(id, &mut out, &mut visited, &mut complete);
        (out, complete)
    }

    /// Census only (bead ita-anc): "what blocks a conclusion" for `id`'s
    /// ancestry, mirroring `lookup_method`/`lookup_singleton`'s walk but
    /// classifying the block instead of returning a method — the
    /// counterfactual `ita check --stats` needs to answer "would this
    /// Inconclusive close if we fixed only project-side opens?" `None`
    /// means this lookup is not ancestry-blocked at all (nothing open,
    /// chain fully resolved).
    ///
    /// External wins unconditionally: an ancestor open because
    /// `declarations/gems.rbi` declared it, or a named ancestor that never
    /// resolved, means closing every project-side open in the chain would
    /// still leave the lookup Inconclusive — real gem knowledge (Tapioca
    /// RBI) is what would close it, not a project-side fix. Otherwise the
    /// FIRST project-side reason in ancestor order (nearest the receiver
    /// class first) is reported — the whole chain is walked (not
    /// short-circuited at the first open ancestor like `lookup_method`)
    /// so a `DeclaredExternal` further down the chain is never missed.
    ///
    /// MEASURED CEILING, and it is not small (bead ita-o1n, 2026-08-21):
    /// this function is per-CLASS and never sees the method name, while
    /// `lookup_method` returns `Found` the moment any ancestor carries the
    /// method — it consults `complete` only after the whole chain misses.
    /// So "External wins" is pessimistic: closing a project-side open can
    /// unblock a site this function labeled `Unresolved`, because the
    /// now-closed ancestor answers the lookup before completeness is ever
    /// Proof: adding `defines_no_method` moved 835 corpus-a call
    /// sites from blind to checked while `anc_dsl` did not budge — every
    /// one of those was booked under `unresolved name`/`declared open`.
    /// Read the project-side buckets as a FLOOR on the closable
    /// population, never as its size. Upgrade path: attribute per
    /// (class, method) at the call site instead of per class.
    ///
    /// ponytail: `singleton` is accepted for symmetry with
    /// `lookup_singleton` (which has its own extra module-on-singleton
    /// Inconclusive exit, unrelated to ancestry) but unused — the
    /// ancestor chain itself is the same walk either way. No memo: this
    /// runs at most once per Inconclusive call site, only under
    /// `Checker::census`, never on the hot check path; add one if a
    /// corpus run shows repeat lookups on the same `id` dominating.
    pub fn inconclusive_reason(&self, id: ClassId, _singleton: bool) -> Option<Blocker> {
        let (ancestors, complete) = self.ancestors(id);
        let mut declared = false;
        let mut first_project: Option<OpenReason> = None;
        for &a in &ancestors {
            let class = self.class(a);
            match class.open_reason {
                Some(OpenReason::DeclaredExternal) => declared = true,
                Some(r) if first_project.is_none() => first_project = Some(r),
                // An ancestor that is `open` with no recorded reason would
                // otherwise vanish from the census and be counted as "not
                // ancestry" — a silent hole. Every `open = true` site sets
                // a reason today, so this stays zero; if it ever moves,
                // `anc_other` is the alarm rather than a wrong total.
                None if class.open && first_project.is_none() => {
                    first_project = Some(OpenReason::Unattributed);
                }
                _ => {}
            }
        }
        // Precedence is the counterfactual, not proximity: each blocker
        // listed here survives the fix for every blocker below it.
        if !complete {
            Some(Blocker::Unresolved)
        } else if declared {
            Some(Blocker::Declared)
        } else {
            first_project.map(Blocker::Project)
        }
    }

    /// Bead ita-k9j: is `id`'s `Blocker::Declared` (if it has one) caused
    /// specifically by the declared-external ancestor named `path`?
    /// Originated (entrega 1) as named-ablation-by-query instead of
    /// remove-and-rebuild (the RUN-LOG 2026-08-22 `ActiveRecord::Base`
    /// ablation numbers); entrega 2 promotes it to a real check-path
    /// predicate too — `Checker::rbi_escalate`'s third step calls this
    /// exact function to decide whether a curated-inventory hit is even
    /// eligible before consulting the name sets. Same precedence as
    /// `inconclusive_reason`: an incomplete chain never counts here even
    /// if `path` is also declared-open somewhere in it, because
    /// `Unresolved` survives fixing every `Declared` entry (see that
    /// function's doc comment). A chain can carry more than one
    /// `declarations/gems.rbi` entry (a model that also mixes in
    /// `Sidekiq::Worker`, say) — this checks PRESENCE of `path` among the
    /// chain's declared-external ancestors, exactly what deleting that
    /// one `gems.rbi` entry would remove.
    pub fn declared_by(&self, id: ClassId, path: &str) -> bool {
        let (ancestors, complete) = self.ancestors(id);
        if !complete {
            return false;
        }
        ancestors.iter().any(|&a| {
            let class = self.class(a);
            class.open_reason == Some(OpenReason::DeclaredExternal) && class.path == path
        })
    }

    fn linearize(
        &self,
        id: ClassId,
        out: &mut Vec<ClassId>,
        visited: &mut std::collections::HashSet<ClassId>,
        complete: &mut bool,
    ) {
        if !visited.insert(id) {
            return; // cycle or diamond: first occurrence wins
        }
        let class = self.class(id);
        let nesting = &class.nesting;
        for name in class.prepends.iter().rev() {
            match self.resolve_const(nesting, name) {
                Some(m) => self.linearize(m, out, visited, complete),
                None => *complete = false,
            }
        }
        out.push(id);
        for name in class.includes.iter().rev() {
            match self.resolve_const(nesting, name) {
                Some(m) => self.linearize(m, out, visited, complete),
                None => *complete = false,
            }
        }
        if let Some(sc) = &class.superclass {
            match self.resolve_superclass_const(id, nesting, sc) {
                Some(s) => self.linearize(s, out, visited, complete),
                None => *complete = false,
            }
        }
        // No written superclass: implicit Object terminator (core table).
    }

    /// Look up an instance method on a project class.
    ///
    /// An ancestor open with `OpenReason::AbstractRaise` no longer blinds
    /// the walk (2026-09-03, rails dd1c8848^): that idiom means "subclasses
    /// complete me", not "anything goes" — the class is treated as closed,
    /// its own methods resolve, and a resulting `NotFound` softens to
    /// `Inconclusive` only when `abstract_family_defines` proves some
    /// member of the RECEIVER's subtree defines `name` (or a fail-closed
    /// refinement fires). The scope is the receiver, never the abstract
    /// ancestor: a bare self-send dispatches on an instance of the
    /// receiver or one of its descendants, so a sibling family member's
    /// open mixin (rails' `Tags::ActionText` pulls in an open
    /// `FormTagHelper`) can never answer a call that dispatches on
    /// `Tags::SearchField` — walking the ancestor's whole family there
    /// would silence the real `request` `NameError` the fix exists to find.
    pub fn lookup_method(&self, id: ClassId, name: &str) -> MethodLookup<'_> {
        let (ancestors, complete) = self.ancestors(id);
        for &a in &ancestors {
            let class = self.class(a);
            if class.open && class.open_reason != Some(OpenReason::AbstractRaise) {
                return MethodLookup::Inconclusive;
            }
            if let Some(m) = class.methods.get(name) {
                return MethodLookup::Found(m, a);
            }
        }
        // A module's instance methods run with `self` = the including
        // class, which we don't know from here: never claim NotFound.
        if complete && !self.class(id).is_module && !self.abstract_family_defines(id, name) {
            MethodLookup::NotFound
        } else {
            MethodLookup::Inconclusive
        }
    }

    /// Does any descendant of `id` define `name`? A self-send inside a
    /// subclassed class dispatches on the RUNTIME class, so a template
    /// method whose hook lives in the subclass resolves fine at runtime —
    /// `NotFound` there is a false positive, not a latent bug. Found by
    /// review of one private benchmark corpus (corpus-a, 2026-08-20): an abstract base whose
    /// template method self-sends three hooks, all three defined by its
    /// single subclass — the only class ever instantiated — and all three
    /// reported as E0101. Three of the six baseline "errors" were wrong
    /// about the code.
    ///
    /// This is the general form of the `NotImplementedError` text scan in
    /// `DefWalker` (which only fires when the author happened to write an
    /// explicit raising stub). It narrows nothing that was already sound:
    /// a leaf class keeps full precision, and a descendant must actually
    /// define the name: the corpus's discriminating case is a class that
    /// IS subclassed where the one subclass inherits the broken method and
    /// never defines the missing identifier either — still an error.
    pub fn descendant_defines(&self, id: ClassId, name: &str, singleton: bool) -> bool {
        let mut stack = vec![id];
        let mut seen = std::collections::HashSet::new();
        while let Some(cur) = stack.pop() {
            if !seen.insert(cur) {
                continue;
            }
            let Some(kids) = self.subclasses.get(&cur) else {
                continue;
            };
            for &kid in kids {
                let class = self.class(kid);
                let map = if singleton {
                    &class.singleton_methods
                } else {
                    &class.methods
                };
                if class.open || map.contains_key(name) {
                    return true;
                }
                stack.push(kid);
            }
        }
        false
    }

    /// The refined instance-track counterpart of `descendant_defines`
    /// (2026-09-03, rails dd1c8848^), walked from the RECEIVER: a base
    /// carrying `raise NotImplementedError` used to be blanket-open,
    /// silencing EVERY instance lookup on it and every descendant — the
    /// mechanism that hid the real production `NameError` on
    /// `ActionView::Helpers::Tags::SearchField#render` (`request` exists
    /// on no member of the whole Tags family). With such classes no longer
    /// blanket-open, a resulting `NotFound` softens to `Inconclusive` only
    /// when this walk proves some member of the receiver's subtree defines
    /// `name` — keyed on the method NAME and the structural descendant
    /// set, never on the class name or a receiver-name pattern. The
    /// subtree IS the runtime dispatch set of a self-send at this
    /// receiver: an ancestor's abstractness never widens it, so a
    /// SIBLING's open mixin cannot silence a lookup it can never answer.
    ///
    /// This replaces the plain `descendant_defines` on the instance track
    /// (the singleton track keeps the original — abstract stubs there
    /// stay blanket-open). Same corpus-a finding as before (2026-08-20,
    /// template hooks supplied by the only instantiated subclass), with
    /// three refinements:
    ///
    /// - a member open for any reason OTHER than `AbstractRaise`
    ///   (`method_missing`, dynamic mixin, non-literal
    ///   `define_method`/`alias_method`, class-body block, dynamic
    ///   superclass, `DeclaredExternal`, ...) could define anything: the
    ///   walk stays `Inconclusive` (refinement (a));
    /// - a member defining `method_missing`/`respond_to_missing?` answers
    ///   every name: silence;
    /// - a closed member's OWN chain must be complete and free of
    ///   non-AbstractRaise opens — `ancestors` walks its includes and
    ///   prepends too, so an open mixin or an unresolved/declared-external
    ///   link under the receiver silences the subtree. The chain above the
    ///   receiver is already proven by the caller (`complete` plus the
    ///   ancestor walk's non-AbstractRaise exit);
    /// - a definition is keyed on the method NAME alone: the member's own
    ///   `methods` map (`def`, literal `define_method`, `attr_*`, literal
    ///   `alias`/`alias_method` all land there as synthetic entries), or
    ///   anything inherited along the member's own chain — a closed module
    ///   a member includes counts, exactly as it would at runtime.
    ///
    /// Members open ONLY via their own `AbstractRaise` pass through: their
    /// bodies call subclass hooks, which is the very idiom being modeled —
    /// their descendants are still walked for the name.
    fn abstract_family_defines(&self, receiver: ClassId, name: &str) -> bool {
        let mut stack = vec![receiver];
        let mut seen = std::collections::HashSet::new();
        while let Some(cur) = stack.pop() {
            if !seen.insert(cur) {
                continue;
            }
            let Some(kids) = self.subclasses.get(&cur) else {
                continue;
            };
            for &kid in kids {
                let class = self.class(kid);
                if class.methods.contains_key(name)
                    || class.methods.contains_key("method_missing")
                    || class.methods.contains_key("respond_to_missing?")
                {
                    return true;
                }
                if class.open {
                    if class.open_reason == Some(OpenReason::AbstractRaise) {
                        stack.push(kid);
                    } else {
                        return true;
                    }
                    continue;
                }
                let (chain, complete) = self.ancestors(kid);
                // The member's full chain can also SUPPLY the name — a
                // closed module it includes defines `request`, say. One
                // pass answers both questions: silence on any definition
                // (the hook resolves at runtime) or any non-AbstractRaise
                // open (the member's surface is unprovable).
                if !complete
                    || chain.iter().any(|&a| {
                        let c = self.class(a);
                        c.methods.contains_key(name)
                            || (c.open && c.open_reason != Some(OpenReason::AbstractRaise))
                    })
                {
                    return true;
                }
                stack.push(kid);
            }
        }
        false
    }

    /// Look up a singleton (class-level) method: singleton methods along the
    /// ancestry, plus instance methods of `extend`ed modules.
    pub fn lookup_singleton(&self, id: ClassId, name: &str) -> MethodLookup<'_> {
        let (ancestors, complete) = self.ancestors(id);
        for &a in &ancestors {
            let class = self.class(a);
            if class.open {
                return MethodLookup::Inconclusive;
            }
            if let Some(m) = class.singleton_methods.get(name) {
                return MethodLookup::Found(m, a);
            }
            if let Some(lookup) = self.extended_module_surface(class, name) {
                return lookup;
            }
        }
        // Bead ita-asx: a class object is an instance of `Class` (a module
        // object, of `Module`, itself a `Class`), so after the singleton
        // chain the lookup ends in `Class`'s INSTANCE surface. A project
        // reopening (`class Class ... end` — the exact shape
        // activesupport's own `core_ext/class/subclasses.rb`,
        // `core_ext/module/introspection.rb` and friends ship) defines
        // those methods in project source, exactly as readable as the
        // `extend`ed module's instance side consulted above: measured on
        // rails, 17 census residue sites (`Parent.descendants`,
        // `module_parent*`, ...) were this shape reading as NotFound. The
        // fragment's openness (`ReopenedExternal`) concerns the REST of
        // the core surface — what the gem may add dynamically — not what
        // the reopening's own text literally defines; this reads only
        // that. Monotonically less diagnostic (NotFound -> Found),
        // invariant #1.
        if let Some(found) = self.core_object_instance_surface(id, name) {
            return found;
        }
        if complete && !self.descendant_defines(id, name, true) {
            MethodLookup::NotFound
        } else {
            MethodLookup::Inconclusive
        }
    }

    /// An `extend M` puts the instance methods of M **and of M's own
    /// ancestry** on the class object's dispatch: resolve the named
    /// module against the ancestor's own nesting and read its whole
    /// linearized chain. An unresolvable module name makes the whole
    /// verdict inconclusive — the surface genuinely cannot be read
    /// (`Some(Inconclusive)`), never silently skipped. Bead ita-asx named
    /// this cluster when the `Class`/`Module` reopening consult
    /// (`core_object_instance_surface`) joined it at the same call site.
    ///
    /// Bead ita-xta (2026-09-21, measured on rails and discourse): reading
    /// only `M`'s OWN `methods` map made `extend ActiveModel::Translation`
    /// — whose whole body is `include ActiveModel::Naming` — read as a
    /// class object with NO `model_name`, a prospective false E0101 on
    /// `Person::Gender.model_name` (`activemodel/test/models/person.rb:18`,
    /// `activemodel/test/cases/translation_test.rb:94`). `extend` is
    /// ordinary Ruby ancestry: the extended module's includes and prepends
    /// dispatch exactly like its own methods. The same walk carries the
    /// fail-closed half — an OPEN ancestor of the extended module (a
    /// `declarations/gems.rbi` entry such as
    /// `ActionView::Helpers::TextHelper`, which discourse's
    /// `Search::GroupedSearchResults::TextHelper` extends, or a project
    /// module this index could not enumerate) makes the surface
    /// unreadable rather than absent, and an incomplete chain does the
    /// same. Monotonic: every verdict this widening changes is
    /// `NotFound` -> `Found`/`Inconclusive`, never the reverse
    /// (invariant #1).
    fn extended_module_surface(&self, class: &ClassDef, name: &str) -> Option<MethodLookup<'_>> {
        for ext in &class.extends {
            let Some(mid) = self.resolve_const(&class.nesting, ext) else {
                return Some(MethodLookup::Inconclusive);
            };
            let (chain, complete) = self.ancestors(mid);
            for &a in &chain {
                let m = self.class(a);
                if m.open {
                    return Some(MethodLookup::Inconclusive);
                }
                if let Some(sig) = m.methods.get(name) {
                    return Some(MethodLookup::Found(sig, a));
                }
            }
            if !complete {
                return Some(MethodLookup::Inconclusive);
            }
        }
        None
    }

    /// The instance surface every class object dispatches through after
    /// its own singleton chain. A class object is an instance of `Class`,
    /// so its dispatch continues `Class` -> `Module` -> `Object` ->
    /// `Kernel` -> `BasicObject`; a module object starts one link later.
    /// Only PROJECT reopenings of those names are consulted — a fragment
    /// exists there only when project source literally wrote
    /// `class Class`/`class Object`/`module Kernel`, and its instance
    /// methods are exactly as readable as an `extend`ed module's. A
    /// missing fragment changes nothing: the builtin core surface is the
    /// inventory's business, not this consult's (bead ita-asx).
    ///
    /// Bead ita-obx (2026-09-21, measured on rails): the chain used to
    /// stop at `Class`/`Module`, so every ActiveSupport core extension
    /// written as `class Object; def in?(...)` or
    /// `class Object; def with(...)` — project source, inside the very
    /// project being checked — read as absent on a class-object receiver.
    /// 10 of rails' 33 residue records were exactly that: `A.in?(B)`
    /// (`activesupport/test/core_ext/object/inclusion_test.rb:51-54`
    /// against that directory's `inclusion.rb:2`) and `X.with(...)` on
    /// four different receivers (against `with.rb:31`). Same monotonic
    /// direction as the `extend` consult above: `NotFound` -> `Found`
    /// only.
    fn core_object_instance_surface(&self, id: ClassId, name: &str) -> Option<MethodLookup<'_>> {
        let object_class: &[&str] = if self.class(id).is_module {
            &["Module", "Object", "Kernel", "BasicObject"]
        } else {
            &["Class", "Module", "Object", "Kernel", "BasicObject"]
        };
        for path in object_class {
            if let Some(&cid) = self.by_path.get(*path) {
                if let Some(m) = self.class(cid).methods.get(name) {
                    return Some(MethodLookup::Found(m, cid));
                }
            }
        }
        None
    }

    /// Bead ita-6bq: does a class's OWN singleton `new` show up somewhere
    /// this checker can actually see, walking the SAME ancestry/`extend`
    /// chain `lookup_singleton` uses — SKIPPING an ancestor whose openness
    /// is caused purely by a `declarations/gems.rbi` reopening
    /// (`OpenReason::DeclaredExternal`, `merge_declared_fragment`'s
    /// force-open)? Plain `lookup_singleton` would stop dead the moment
    /// it hits ANY open ancestor, gem-declared or project-side alike —
    /// correct for a general singleton-method dispatch, but too coarse
    /// for THIS specific question. Bead ita-k9j (entrega 2) already
    /// established the precedent this generalizes: a project subclass's
    /// OWN `initialize` governs `.new`'s arity even when the ancestry
    /// passes through a force-opened `ActiveRecord::Base` — that gem stub
    /// carries zero real methods by contract, so it is exactly as silent
    /// on `self.new` as it is on everything else, never a reason to
    /// distrust the project's own code. Measured regression without this
    /// carve-out: EVERY `ActiveRecord` model's `.new` arity check would go
    /// silently `Inconclusive`, because `ActiveRecord::Base` sits in
    /// every one of their ancestor chains — the checker's single most
    /// common receiver shape in a Rails corpus, not a rare edge case.
    ///
    /// A PROJECT-side open ancestor (unresolved DSL, dynamic mixin, a
    /// class-body block like `instance_methods.each { |m| undef_method
    /// m }` — bead ita-d0j, the actual `rails/activesupport`
    /// `DeprecationProxy` shape) still stops the walk and returns
    /// `Inconclusive`: that ancestor's real surface — possibly including
    /// a genuine `self.new` override this checker cannot see — is
    /// exactly the risk `.new`'s Found-branch above exists to catch, and
    /// skipping it would silently reopen the false positive this bead was
    /// filed to fix in the first place.
    ///
    /// The chain's own `complete` flag (an ancestor NAME that never
    /// resolved anywhere in the project's own index — e.g. `class Foo <
    /// SomeGem::Base` where only a Tapioca RBI, not project code,
    /// declares `SomeGem::Base`) is deliberately NOT consulted for the
    /// final `NotFound` verdict either, for the same "external, not
    /// project-suspicious" reasoning as the `DeclaredExternal` skip above
    /// — this checker's own `external_ancestor_starts` already groups
    /// unresolved-name ancestors and `DeclaredExternal` ancestors as the
    /// SAME population for RBI-escalation purposes. Bead ita-xze's own
    /// fixtures (`rbi_call_resolution.rs`) pin the precedent: a project
    /// class whose superclass name is external-and-unresolved still
    /// resolves `.new` through its OWN directly-defined `initialize`,
    /// exactly as it did before this bead — an unresolved ancestor name
    /// no more hides a project-relevant `self.new` than a declared one
    /// does. A genuinely OPEN project-side ancestor is still caught above
    /// (that check runs first, over every ancestor this function DOES
    /// walk) — this only widens what "no further ancestors to check"
    /// means once the walk runs off the end of a resolvable chain.
    pub fn lookup_singleton_own(&self, id: ClassId, name: &str) -> MethodLookup<'_> {
        let (ancestors, _complete) = self.ancestors(id);
        for &a in &ancestors {
            let class = self.class(a);
            if class.open {
                if class.open_reason == Some(OpenReason::DeclaredExternal) {
                    continue;
                }
                return MethodLookup::Inconclusive;
            }
            if let Some(m) = class.singleton_methods.get(name) {
                return MethodLookup::Found(m, a);
            }
            for ext in &class.extends {
                if let Some(mid) = self.resolve_const(&class.nesting, ext) {
                    if let Some(m) = self.class(mid).methods.get(name) {
                        return MethodLookup::Found(m, mid);
                    }
                } else {
                    return MethodLookup::Inconclusive;
                }
            }
        }
        if self.descendant_defines(id, name, true) {
            MethodLookup::Inconclusive
        } else {
            MethodLookup::NotFound
        }
    }

    /// `lookup_method` with the two ita-4xy follow-up fallbacks applied
    /// to a `NotFound` verdict (bead ita-4xy carve-out closed classes
    /// that used to stay `open`, unmasking two pre-existing lookup gaps
    /// — see `soften_not_found`'s doc comment). Every other outcome is
    /// `lookup_method`'s own, unchanged: this only ever turns an
    /// existing `NotFound` into `Inconclusive`, monotonically LESS
    /// diagnostic (invariant #1), never the reverse. `rbi_map` mirrors
    /// every other RBI-aware call (`rbi_method_lookup`, `rbi_declares`):
    /// `None` when no client `sorbet/rbi` was discovered, degrading
    /// byte-for-byte to `lookup_method`'s own behavior.
    pub fn lookup_method_rbi(
        &self,
        id: ClassId,
        name: &str,
        rbi_map: Option<&HashMap<String, Vec<PathBuf>>>,
    ) -> MethodLookup<'_> {
        match self.lookup_method(id, name) {
            MethodLookup::NotFound => self.soften_not_found(id, name, false, rbi_map),
            other => other,
        }
    }

    /// Singleton counterpart of `lookup_method_rbi` — see that
    /// function's and `soften_not_found`'s doc comments.
    pub fn lookup_singleton_rbi(
        &self,
        id: ClassId,
        name: &str,
        rbi_map: Option<&HashMap<String, Vec<PathBuf>>>,
    ) -> MethodLookup<'_> {
        match self.lookup_singleton(id, name) {
            MethodLookup::NotFound => self.soften_not_found(id, name, true, rbi_map),
            other => other,
        }
    }

    /// The two NotFound-softening fallbacks (bead ita-4xy follow-up),
    /// consulted only after `lookup_method`/`lookup_singleton`'s own
    /// walk — `descendant_defines` included — already committed to
    /// `NotFound`:
    ///
    /// 1. Gem-reopening (`gem_reopens`): the project's own index sees
    ///    `id`'s ancestry as fully closed, but SOME ancestor in that
    ///    closed chain — `id` itself, or any class/module reached via a
    ///    fully-resolved include/prepend/superclass link — is a
    ///    reopening of a class whose PRIMARY definition lives in a gem
    ///    (`class ::Foo` where a Tapioca RBI also declares `Foo`) — the
    ///    project's view of THAT ancestor is provably partial, so
    ///    `NotFound` can never be proven here. Bead ita-oaq (2026-08-25,
    ///    measured against ruby-lsp's own test suite): the original
    ///    version of this check looked at `id` alone, which misses the
    ///    common two-hop shape `class FooTest < TestCase` where
    ///    `TestCase < Minitest::Test` and the PROJECT itself reopens
    ///    `module Minitest; class Test; include ProjectHelper; end; end`
    ///    (a common "mix project test helpers into the gem's base test
    ///    class" pattern) — `Minitest::Test` resolves as a genuine,
    ///    CLOSED project ancestor two links up from the receiver, so the
    ///    chain looks fully closed and `assert_equal`/every Minitest
    ///    DSL method the project itself never redeclares reports a
    ///    fabricated E0101, though the same gem RBI that would have
    ///    silenced an `unresolved name` ancestor is sitting right there
    ///    under `Minitest::Test`'s own path. Typed resolution, when the
    ///    RBI's own sig maps one, still happens with NO new BFS:
    ///    `check.rs::Checker::rbi_escalate` (already wired on every
    ///    `Inconclusive` outcome) tries `dsl_method_lookup` FIRST, whose
    ///    `project_ancestor_starts` already seeds EVERY ancestor's own
    ///    path in MRO order — `rbi_map` is one flat `constant -> file`
    ///    map over the whole `sorbet/rbi` tree (gems, dsl, annotations
    ///    alike), so a gem RBI reopening any ancestor's exact path
    ///    resolves through that EXISTING walk exactly as if it were a
    ///    DSL reopening. Returning `Inconclusive` here is the only thing
    ///    this bead needed to add.
    /// 2. Kernel/Object (`core::kernel_object_instance_method`/
    ///    `core::kernel_object_singleton_method`): every Ruby object (an
    ///    instance query) or class object (a singleton query — `def
    ///    self.x` bodies) answers to Kernel's own surface (`proc`,
    ///    `lambda`, ...) and Object/BasicObject's (plus Class/Module's,
    ///    singleton side) own methods, regardless of whether the
    ///    receiving project class is closed. A bare self-send of one of
    ///    these inside a closed class's method body is never a real
    ///    `NotFound`. No `Ty` is ever modeled for a hit here (unlike gap
    ///    1, no RBI sig exists to map) — `Inconclusive` (silence) is the
    ///    correct, final answer.
    /// 3. Dynamic mixin (bead ita-o8l.1, replaces ita-a8z): `name` is
    ///    directly defined by a module the project dynamically mixes in
    ///    through a receiver this checker can never resolve
    ///    (`builder_class.include(Module)`). See `ProjectIndex::
    ///    dynamic_mixin_covers`'s doc comment for the exact rule
    ///    (including the measured-and-abandoned `method_missing`
    ///    clause) — unlike gaps 1/2 this is keyed on the METHOD NAME,
    ///    never on `id`/the receiving class.
    fn soften_not_found(
        &self,
        id: ClassId,
        name: &str,
        singleton: bool,
        rbi_map: Option<&HashMap<String, Vec<PathBuf>>>,
    ) -> MethodLookup<'_> {
        if rbi_map.is_some_and(|map| self.gem_reopens(id, map)) {
            return MethodLookup::Inconclusive;
        }
        let kernel_hit = if singleton {
            // bead ita-tail: the bare-call private tail (`raise`, `rand`,
            // ...) — a class object really answers these; NotFound on one is
            // a prospective FALSE E0101 (79-97% of the measured residue).
            // Still NotFound-only: `lookup_singleton_own` already returned,
            // so a project-defined `raise` won above.
            core::kernel_object_singleton_method(name) || core::kernel_bare_call_method(name)
        } else {
            core::kernel_object_instance_method(name)
        };
        if kernel_hit {
            return MethodLookup::Inconclusive;
        }
        let softened = if singleton {
            self.singleton_surface_softens(id, name)
        } else {
            // The instance-side dynamic mixin: a module mixed into
            // instances by a runtime `include`/`prepend` the walker could
            // not resolve, keyed on the method name.
            self.dynamic_mixin_covers(&self.dynamic_mixin_instance_targets, name)
        };
        if softened {
            return MethodLookup::Inconclusive;
        }
        MethodLookup::NotFound
    }

    /// The class-object-only softenings, split from `soften_not_found` so
    /// the parent stays under the complexity ceiling. Order and
    /// short-circuit are identical to the inline sequence they replaced;
    /// each arm keeps the measurement that justifies it. Softening only —
    /// never a signature, so no arity is manufactured from a method this
    /// index never read.
    fn singleton_surface_softens(&self, id: ClassId, name: &str) -> bool {
        // Singleton-track family (e): the stdlib's own class-object
        // surface. `FileUtils.mkdir_p`, `SecureRandom.uuid`, `Kernel.rand`
        // are real methods living in no project index — 270 of discourse's
        // explicit-receiver residue sites and the single largest family
        // left. Mechanically harvested (`declarations/stdlib_singletons.txt`).
        if stdlib_singleton_method(&self.class(id).path, name) {
            return true;
        }
        // Bead ita-sgl: the stdlib `Singleton` mixin. `include Singleton`
        // runs `Singleton.included(klass)`, whose body does
        // `klass.extend SingletonClassMethods` — so the includer's CLASS
        // OBJECT gains exactly `instance`, `_load` and `clone`. DOUBLY
        // KEYED, never blanket: the receiver's own resolved ancestry must
        // contain the constant `Singleton`, AND the name must be one of the
        // three that mixin really installs.
        if matches!(name, "instance" | "_load" | "clone") && self.includes_singleton_mixin(id) {
            return true;
        }
        // The mocking gems' class-object surface (`X.any_instance`):
        // name-keyed, lock-gated (see `apply_mock_singleton_surface`).
        // Receiver-blind by construction, so ONLY the name may carry the
        // proof, and only while this project's lock declares a gem that
        // really installs it.
        if self.mock_singleton_methods.iter().any(|m| m == name) {
            return true;
        }
        // The dynamic singleton mixin: a module extended onto the class
        // object by a runtime call the walker could not resolve, keyed on
        // the method name.
        self.dynamic_mixin_covers(&self.dynamic_mixin_singleton_targets, name)
    }

    /// Bead ita-sgl: does `id`'s resolved ancestry include the constant
    /// `Singleton`? Exact top-level path equality on a RESOLVED ancestor,
    /// never a text match on the `includes` list: a module actually named
    /// `Singleton` is what `Singleton.included`'s hook hangs off, and an
    /// ancestor name that never resolved already makes the chain
    /// incomplete (`lookup_singleton` returns `Inconclusive` before this
    /// is ever reached).
    fn includes_singleton_mixin(&self, id: ClassId) -> bool {
        let (chain, _complete) = self.ancestors(id);
        chain.iter().any(|&a| self.class(a).path == "Singleton")
    }

    /// Bead ita-o8l.1: does an about-to-be-`NotFound` lookup for `name`
    /// soften, because `name` is directly defined by a module the
    /// project dynamically mixes in? `targets` is
    /// `dynamic_mixin_instance_targets` or
    /// `dynamic_mixin_singleton_targets` (the caller, `soften_not_found`,
    /// picks by track). Deliberately shallow: only the target module's
    /// OWN `methods` map, never that module's own ancestry — the
    /// audited shape needs nothing deeper (`Rails::ActionMethods`'s
    /// `attr_reader :options` is a direct, literal method), and a
    /// second unbounded chase through an arbitrary module's own mixins
    /// risks silencing far more than this bead measured. Keyed on
    /// `name` alone, never on the receiving class `id` — see
    /// `FileScan`'s doc comment for why ita-a8z's two
    /// class-keyed candidates were both rejected.
    ///
    /// ABANDON, measured: the task's third clause — soften whenever
    /// some target module merely DEFINES `method_missing`, regardless
    /// of `name` — was built, then five-corpus-measured, then reverted.
    /// It is NOT narrower than the rejected `Global` candidate: any
    /// real corpus with even ONE dynamically-mixed `method_missing`
    /// target UNRELATED to this cluster (rails has two — `ActiveRecord
    /// ::TestFixtures` on the instance track, `ActionDispatch::
    /// Integration::Runner` on the singleton track, both legitimate,
    /// neither anything to do with `Rails::ActionMethods`) silences
    /// EVERY project-wide `NotFound` on that track — this checker has
    /// no way to scope by receiving class (forbidden by this bead's own
    /// design) or by which dynamic-`include` call site is actually
    /// reachable from a given receiver. Measured against a freshly
    /// built binary: enabling the clause silenced 222/222 (100%) of
    /// rails' baseline E0101 and 207/207 (100%) of zammad's — the exact
    /// failure shape `Global` was rejected for, at a larger scale.
    /// Direct-name-only, without it: 41 rails + 7 discourse = 48 sites
    /// silenced project-wide, zero appeared, and the 8 `class_eval`-
    /// generated `Rails::ActionMethods` forwarding methods (`template`,
    /// `copy_file`, `directory`, `empty_directory`, `inside`,
    /// `empty_directory_with_keep_file`, `create_file`, `chmod`,
    /// `shebang` — string-interpolated inside a heredoc passed to
    /// `class_eval`, invisible to any static per-`def`-node scan) stay
    /// a documented false negative (invariant #1 permits this; a
    /// fabricated diagnostic is never permitted).
    fn dynamic_mixin_covers(&self, targets: &[ClassId], name: &str) -> bool {
        targets.iter().any(|&mid| self.class(mid).methods.contains_key(name))
    }

    /// Gap 1's detection signal: does the client's Tapioca gem RBI tree
    /// ALSO declare `id`'s OWN fully-qualified path, or the path of any
    /// ancestor reached by walking `id`'s already-closed, already-
    /// resolved MRO (`ancestors`, which includes `id` itself first) —
    /// i.e. is `id`, or some class/module it inherits from or mixes in,
    /// a project reopening of a class primarily defined in a gem, not a
    /// genuinely project-original one? Bead ita-oaq extends this from
    /// "`id` alone" to the whole chain: `soften_not_found` only ever
    /// calls this after `lookup_method`/`lookup_singleton` already
    /// walked the SAME chain and found it fully resolved (`complete`)
    /// and nowhere `open`, so re-walking it here costs one more cheap
    /// `ancestors` call, never a project-wide scan — see that function's
    /// doc comment (gap 1) for the false-positive shape this closes.
    /// `rbi_map` is `RbiProject`'s phase-1 `constant name -> EVERY
    /// declaring file` map (`rbi.rs::build_rbi_index`), TOP-LEVEL names
    /// only — reliable for a top-level constant (Tapioca always writes
    /// the fully qualified name on the `class`/`module` header line),
    /// but known to miss a NESTED name coming from `sorbet/rbi/dsl/`
    /// (bead ita-iyz: the phase-1 scanner has no indentation tracking,
    /// so a nested `module Foo::Bar` inside a DSL file can be recorded
    /// under the wrong key or missed). Every ancestor's `path` here is
    /// always that class's own written name (`Foo`, `Foo::Bar`, ...), so
    /// both cases are safe: the only failure mode on the nested/DSL side
    /// is a false NEGATIVE (this fallback simply doesn't fire for that
    /// one ancestor, exactly as if this bead didn't exist), never a
    /// false positive. `contains_key` alone (bead ita-k9j.3): whether
    /// the map now holds one file or several for a name changes nothing
    /// here — this only asks IF a gem reopens the ancestor's path, never
    /// which file, so the Vec value type is unaffected.
    fn gem_reopens(&self, id: ClassId, rbi_map: &HashMap<String, Vec<PathBuf>>) -> bool {
        let (chain, _complete) = self.ancestors(id);
        chain.iter().any(|&a| rbi_map.contains_key(&self.class(a).path))
    }

    /// Scan `ancestors[start..]`, applying the same `open`/`Found`/
    /// `NotFound` rule as `lookup_method`/`lookup_singleton`, just from an
    /// arbitrary starting position instead of the chain's head. Shared by
    /// `super_lookup` (bead ita-53y): `super` never restarts the chain, it
    /// continues from wherever the currently executing method actually
    /// sits.
    fn lookup_from(
        &self,
        ancestors: &[ClassId],
        complete: bool,
        start: usize,
        name: &str,
        singleton: bool,
    ) -> MethodLookup<'_> {
        for &a in ancestors.iter().skip(start) {
            let class = self.class(a);
            if class.open {
                return MethodLookup::Inconclusive;
            }
            let map = if singleton {
                &class.singleton_methods
            } else {
                &class.methods
            };
            if let Some(m) = map.get(name) {
                return MethodLookup::Found(m, a);
            }
        }
        if complete {
            MethodLookup::NotFound
        } else {
            MethodLookup::Inconclusive
        }
    }

    /// `super`/`super(...)` navigation (bead ita-53y): resolve to the
    /// ancestor located strictly after the one that physically defines the
    /// currently executing method — never the start of the chain, and
    /// never a guess. A method defined directly on a class continues into
    /// that class's own superclass chain. A method defined inside a module
    /// reached only via `include`/`prepend` has no chain of its own: it
    /// continues into every class in the whole project whose ancestry
    /// actually mixes the module in, starting right after the module's
    /// slot in that specific class's MRO. If two consuming classes would
    /// answer differently, the module is consumed nowhere in the project,
    /// or any relevant chain is incomplete: silence — never a wrong guess.
    ///
    /// ponytail: singleton `super` reached only through `extend` isn't
    /// modeled — `extend` doesn't sit inside the `ancestors()` chain at
    /// all (see `lookup_singleton`), so "the next ancestor" isn't well
    /// defined there; add it when a fixture needs it.
    pub fn super_lookup(&self, owner: ClassId, singleton: bool, name: &str) -> MethodLookup<'_> {
        if self.class(owner).is_module {
            return if singleton {
                MethodLookup::Inconclusive
            } else {
                self.super_lookup_module(owner, name)
            };
        }
        let (ancestors, complete) = self.ancestors(owner);
        match ancestors.iter().position(|&a| a == owner) {
            Some(pos) => self.lookup_from(&ancestors, complete, pos + 1, name, singleton),
            None => MethodLookup::Inconclusive,
        }
    }

    /// The module branch of `super_lookup`, split out to stay under the
    /// complexity ceiling: scans every class in the project for one whose
    /// ancestry actually mixes `module` in, and continues that specific
    /// class's linearization from right after the module's slot.
    fn super_lookup_module(&self, module: ClassId, name: &str) -> MethodLookup<'_> {
        let mut outcome: Option<Result<(&MethodSig, ClassId), ()>> = None;
        for i in 0..self.classes.len() {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "class-table index bounded by `self.classes.len()`, far below u32::MAX for any real Ruby project"
            )]
            let consumer = ClassId(i as u32);
            if self.class(consumer).is_module {
                continue;
            }
            let (ancestors, complete) = self.ancestors(consumer);
            let Some(pos) = ancestors.iter().position(|&a| a == module) else {
                continue;
            };
            let here = match self.lookup_from(&ancestors, complete, pos + 1, name, false) {
                MethodLookup::Inconclusive => return MethodLookup::Inconclusive,
                MethodLookup::NotFound => Err(()),
                MethodLookup::Found(m, a) => Ok((m, a)),
            };
            if !outcomes_agree(outcome, here) {
                return MethodLookup::Inconclusive;
            }
            outcome = Some(here);
        }
        match outcome {
            Some(Ok((m, a))) => MethodLookup::Found(m, a),
            Some(Err(())) => MethodLookup::NotFound,
            // The module is never actually mixed into any indexed class:
            // no consumer chain to continue from, so no navigable target.
            None => MethodLookup::Inconclusive,
        }
    }
    /// Bead ita-54k: does `name`, referenced from lexical `nesting`, name
    /// a constant written as an alias (`X = Y`, or qualified `A::B =
    /// C::D`, RHS itself a literal constant path — see
    /// `ProjectIndex::const_aliases`)? Mirrors `resolve_const`'s own
    /// candidate generation (innermost nesting level outward, then the
    /// bare name) so a bare alias reference resolves from the SAME
    /// candidate scopes a real class reference would — just looked up in
    /// `const_aliases` instead of `by_path`. Suppression-only:
    /// `resolve_const` itself never calls this, and never will (widening
    /// it could manufacture a false type per invariant #1).
    fn find_const_alias(&self, nesting: &[String], name: &str) -> Option<&(Vec<String>, String)> {
        if let Some(rest) = name.strip_prefix("::") {
            return self.const_aliases.get(rest);
        }
        for level in nesting.iter().rev() {
            let candidate = format!("{level}::{name}");
            if let Some(alias) = self.const_aliases.get(&candidate) {
                return Some(alias);
            }
        }
        self.const_aliases.get(name)
    }

    /// ponytail: generous ceiling for an alias chain — no real idiom
    /// (`X = SomeGem::Y`) is longer than one or two hops; raise only if a
    /// real fixture needs more.
    const CONST_ALIAS_CHAIN_CAP: usize = 32;

    /// Bead ita-54k: does `name` resolve to a real project class by
    /// following one or more constant-alias hops (`X = Y`, `Y = Z`, ...)?
    /// Each hop resolves the RHS from ITS OWN write-site lexical scope
    /// (`find_const_alias`'s stored nesting), not the original
    /// reference's — matching real Ruby, where an assignment's
    /// right-hand side is evaluated once, at definition time, in
    /// whatever scope wrote it. `visited` guards a cycle (`X = Y; Y =
    /// X`); `CONST_ALIAS_CHAIN_CAP` guards a pathologically long chain.
    /// Either exit degrades to `None` — a silent miss, never a panic or
    /// an infinite loop — because this is consulted only as a fallback
    /// inside `const_exists`, which only ever SUPPRESSES E0104 on a hit
    /// (invariant #1: a wrong miss here costs a warning, never a false
    /// diagnostic).
    fn resolve_const_via_alias(&self, nesting: &[String], name: &str) -> Option<ClassId> {
        let mut cur_nesting = nesting.to_vec();
        let mut cur_name = name.to_string();
        let mut visited = std::collections::HashSet::new();
        for _ in 0..Self::CONST_ALIAS_CHAIN_CAP {
            if !visited.insert(cur_name.clone()) {
                return None; // cycle
            }
            let (next_nesting, target) = self.find_const_alias(&cur_nesting, &cur_name)?.clone();
            if let Some(id) = self.resolve_const(&next_nesting, &target) {
                return Some(id);
            }
            cur_nesting = next_nesting;
            cur_name = target;
        }
        None
    }

    /// Bead ita-47y: resolve a constant path through literal-alias hops
    /// at ANY `::`-segment, not only the last-but-one
    /// `resolve_const_via_alias` chases. The measured ruby-lsp shape
    /// (`Interface = LanguageServer::Protocol::Interface`, then
    /// `Interface::CompletionItemKind::FIELD` — TWO segments past the
    /// alias, not one) never resolved through `const_exists`'s qualified
    /// branch: that branch only ever tries an alias on the text
    /// immediately left of the FINAL `::`, so a reference two-or-more
    /// segments past the alias always fell through to
    /// `toplevel_consts`/`stdlib_declares` and warned a false E0104 even
    /// though the alias itself, and a one-segment-nested reference
    /// through it, already resolved.
    ///
    /// Unlike `resolve_const_via_alias` (suppression-only, `const_exists`'s
    /// private fallback, never returns a type), this returns a real
    /// `ClassId` — the SAME class a direct reference to the alias's
    /// target would resolve to — so a caller (`check.rs::infer_const`)
    /// can type the reference `Ty::Class(id)` exactly as if the alias had
    /// never been in the way. That is what both kills the false E0104 AND
    /// legitimately unlocks E0101/E0103 for a bogus member reached
    /// through the alias (a class this checker now knows is closed can
    /// diagnose a wrong method call on it, same as any other resolved
    /// class) — the anti-suppression half of invariant #1 still holds:
    /// only a segment that genuinely resolves ever produces a `ClassId`,
    /// so a wrong member past a real alias keeps accusing exactly like a
    /// wrong member past a real class would.
    ///
    /// Ruby only ever lexically searches the FIRST segment of a constant
    /// path expression from the reference site's nesting; every later
    /// segment is a literal child lookup of whatever the previous segment
    /// resolved to (never re-searched lexically) — so only the first
    /// segment tries `resolve_const`'s nesting walk (falling back to
    /// `resolve_const_via_alias` for that one segment, exactly
    /// `const_exists`'s existing single-hop behavior), and every
    /// following segment is looked up as an EXACT qualified path
    /// (`resolve_alias_segment`), itself falling back to
    /// `resolve_const_via_alias` keyed by that exact qualified name for
    /// an interior alias hop (`Owner::Seg = ...`).
    ///
    /// Still literal-path-only (ita-exc): every hop chased here was
    /// already recorded by `const_aliases`, which only ever records a
    /// RHS that parsed as a bare/qualified constant path — nothing here
    /// widens WHAT counts as an alias, only how many `::`-segments past
    /// one a caller may walk. `resolve_const` itself stays untouched and
    /// lexical-only, per its own doc comment. A miss at any segment
    /// degrades to `None` — a silent miss, never a panic — the same
    /// failure mode `resolve_const`/`resolve_const_via_alias` already
    /// have.
    pub fn resolve_const_through_aliases(&self, nesting: &[String], name: &str) -> Option<ClassId> {
        if let Some(id) = self.resolve_const(nesting, name) {
            return Some(id);
        }
        let (first_nesting, first) = match name.strip_prefix("::") {
            Some(rest) => (&[][..], rest),
            None => (nesting, name),
        };
        let mut segments = first.split("::");
        let first_seg = segments.next()?;
        let mut owner = self
            .resolve_const(first_nesting, first_seg)
            .or_else(|| self.resolve_const_via_alias(first_nesting, first_seg))?;
        for seg in segments {
            owner = self.resolve_alias_segment(owner, seg)?;
        }
        Some(owner)
    }

    /// One interior `::`-segment step past an already-resolved `owner`
    /// (bead ita-47y): a plain child lookup (`by_path`), or — if the
    /// exact qualified name is itself a literal-path alias write —
    /// chased via `resolve_const_via_alias` keyed by that exact qualified
    /// path. No lexical search: see
    /// `resolve_const_through_aliases`'s doc comment for why only the
    /// FIRST segment of a path expression is ever lexically searched.
    fn resolve_alias_segment(&self, owner: ClassId, seg: &str) -> Option<ClassId> {
        let qualified = format!("{}::{seg}", self.class(owner).path);
        if let Some(&id) = self.by_path.get(&qualified) {
            return Some(id);
        }
        self.resolve_const_via_alias(&[], &format!("::{qualified}"))
    }

    /// Bead ita-47y (RBI-target extension): fully chase a literal-alias
    /// chain and return the FINAL target TEXT — unlike
    /// `resolve_const_via_alias` (`Option<ClassId>`, a silent miss the
    /// moment a target never resolves to a project `ClassId`), this
    /// keeps going until it finds a leaf that is NOT itself a further
    /// alias, and hands that leaf's raw spelling back so a caller can
    /// check it against something OTHER than this project's own index —
    /// specifically a client's vendorized `sorbet/rbi` (the measured
    /// ruby-lsp shape: `Interface = LanguageServer::Protocol::Interface`,
    /// whose real target is declared only in
    /// `sorbet/rbi/gems/language_server-protocol@*.rbi`, never as a
    /// project `ClassId`). Returns `None` when `name` is not an alias at
    /// all, the chain resolves to a real project `ClassId` after all (not
    /// this fallback's job — `resolve_const_through_aliases` already
    /// covers that), or the chain cycles.
    fn chase_alias_target_text(&self, nesting: &[String], name: &str) -> Option<String> {
        let mut cur_nesting = nesting.to_vec();
        let mut cur_name = name.to_string();
        let mut visited = std::collections::HashSet::new();
        for _ in 0..Self::CONST_ALIAS_CHAIN_CAP {
            if !visited.insert(cur_name.clone()) {
                return None; // cycle
            }
            let (next_nesting, target) = self.find_const_alias(&cur_nesting, &cur_name)?.clone();
            if self.resolve_const(&next_nesting, &target).is_some() {
                return None; // resolves in-project after all: not this fallback's job
            }
            if self.find_const_alias(&next_nesting, &target).is_none() {
                return Some(target); // leaf: no further alias hop past this target
            }
            cur_nesting = next_nesting;
            cur_name = target;
        }
        None
    }

    /// Bead ita-47y (RBI-target extension): like
    /// `resolve_const_through_aliases`, walk `name` one `::`-segment at a
    /// time (only the FIRST segment lexically searched, every later
    /// segment a literal child of whatever came before) — but instead of
    /// stopping at the first segment this project's own index cannot
    /// resolve, check whether THAT exact segment is itself a literal
    /// alias whose target ALSO never resolves in-project
    /// (`chase_alias_target_text`). If so, return the alias's raw target
    /// text concatenated with every remaining `::`-segment — the exact
    /// expanded path `Checker::check_const_ref` should retry against a
    /// client's `sorbet/rbi` next. `None` when no alias is involved at
    /// the failing segment at all (a genuinely undefined project
    /// constant, unrelated to this extension) — the caller only reaches
    /// this after `resolve_const_through_aliases`/`const_exists` already
    /// failed, so this can only ever ADD a suppression, never remove one
    /// (invariant #1).
    pub fn expand_unresolved_alias_target(&self, nesting: &[String], name: &str) -> Option<String> {
        let (first_nesting, first) = match name.strip_prefix("::") {
            Some(rest) => (&[][..], rest),
            None => (nesting, name),
        };
        let mut segments = first.split("::");
        let first_seg = segments.next()?;
        let Some(mut owner) = self.resolve_const(first_nesting, first_seg) else {
            let target = self.chase_alias_target_text(first_nesting, first_seg)?;
            return Some(join_remaining(&target, segments));
        };
        for seg in segments.by_ref() {
            let qualified = format!("{}::{seg}", self.class(owner).path);
            if let Some(&id) = self.by_path.get(&qualified) {
                owner = id;
                continue;
            }
            let target = self.chase_alias_target_text(&[], &format!("::{qualified}"))?;
            return Some(join_remaining(&target, segments));
        }
        None // every segment resolved in-project after all — nothing to expand
    }


    /// Is `name` resolvable as a constant from `scope`? Checks class paths
    /// and plain `CONST = ...` definitions, in each lexical scope and then
    /// in that scope's ancestors — Ruby's real order is lexical nesting
    /// first, then the ancestors of the innermost cref, and skipping the
    /// second half made every inherited or mixed-in constant look
    /// unresolved (a constant reached through an `include`d sibling module;
    /// a constant defined on the superclass).
    ///
    /// Only ever used to SUPPRESS E0104, never to produce a type: a hit
    /// here makes strictly fewer diagnostics, so widening it cannot invent
    /// a false positive. `resolve_const` deliberately stays lexical-only,
    /// because widening *it* would turn `Ty::Unknown` into `Ty::Class`
    /// and could manufacture a new E0101/E0102/E0103 (invariant #1).
    ///
    /// Bead ita-54k: when the qualified branch's prefix fails to resolve
    /// via `resolve_const`, it also tries `resolve_const_via_alias` — the
    /// prefix itself may be a constant alias (`X = Y`) rather than a real
    /// class, and a reference through it (`X::Something`) must resolve
    /// exactly as `Y::Something` would. Bead ita-47y widens this to
    /// `resolve_const_through_aliases`, which also chases a prefix that is
    /// itself MULTIPLE segments past the alias (`X::Y::Something`, not
    /// only `X::Something`) — see that function's doc comment.
    pub fn const_exists(&self, nesting: &[String], name: &str) -> bool {
        if self.resolve_const_through_aliases(nesting, name).is_some() {
            return true;
        }
        // Plain constants: check simple-name membership in the owning class
        // of each lexical candidate scope.
        let (owner, simple) = match name.rsplit_once("::") {
            Some((prefix, simple)) => {
                // Bead ita-yh1: DO NOT trim a leading `::` off `prefix`
                // here — `resolve_const_through_aliases`/`resolve_const`
                // treat a leading `::` as "top level only" (see
                // `resolve_const`'s `strip_prefix("::")`), which is exactly
                // real Ruby cbase semantics. Stripping it before this
                // lookup let a same-named lexically-nested module SHADOW
                // the real top-level owner (`::Tapioca::TAPIOCA_DIR`
                // resolving `Tapioca` to a nested `RubyLsp::Tapioca`
                // instead of the top-level `::Tapioca`), producing a false
                // E0104. A `prefix` with no leading `::` is unaffected —
                // this only changes behavior for a genuinely cbase-
                // prefixed `name`.
                let owner = self.resolve_const_through_aliases(nesting, prefix);
                let Some(owner) = owner else {
                    // Bead ita-exc defect B, third shape: an unresolved
                    // owner is exactly the case `resolve_qualified_const_writes`
                    // falls back to `toplevel_consts` for, keyed by the
                    // full written path — check it before giving up on
                    // the project side entirely.
                    //
                    // Bead ita-9he: an EMPTY `prefix` here only ever comes
                    // from a cbase single-segment reference (`::X`,
                    // rsplit_once("::") on `"::X"` yields `("", "X")` —
                    // see this arm's own doc comment above). That is the
                    // read-side twin of the write-side fix: `toplevel_consts`
                    // is keyed by the BARE simple name for a true top-level
                    // constant (both the `ConstantWriteNode` arm's
                    // `frag_idx == None` case and the new cbase
                    // `ConstantPathWriteNode` case push the bare name, never
                    // a `::`-prefixed one), so looking up the untrimmed
                    // `name` (`"::X"`) here would always miss even after a
                    // matching write was indexed. A non-empty `prefix`
                    // (`Foo::Bar` where `Foo` doesn't resolve) is the
                    // pre-existing qualified-write shape and is untouched:
                    // `resolve_qualified_const_writes` keys THAT fallback
                    // as the full `"owner::simple"` text, which `name`
                    // already equals for that shape.
                    return self.toplevel_consts.contains(if prefix.is_empty() { simple } else { name })
                        || stdlib_declares(&self.requires, nesting, name);
                };
                (Some(owner), simple)
            }
            None => (None, name),
        };
        if let Some(owner) = owner {
            self.const_in_ancestors(owner, simple)
                // W3: stdlib fallback runs after every project-side
                // check failed — see `stdlib_declares`.
                || stdlib_declares(&self.requires, nesting, name)
        } else {
            // Bead ita-519: real nesting levels, innermost first —
            // never a string-truncation walk over a flat scope (see
            // `resolve_const`'s doc comment for why that over-widens
            // past compact-syntax class/module boundaries).
            for level in nesting.iter().rev() {
                if let Some(&id) = self.by_path.get(level) {
                    if self.const_in_ancestors(id, simple) {
                        return true;
                    }
                }
            }
            // Bead ita-exc defect B: every real lexical scope has now
            // been consulted and none carried `simple`. This is the
            // TERMINAL step — a true toplevel constant was
            // unreachable before this bucket existed. A hit only
            // suppresses E0104, same contract as `stdlib_declares`
            // right beside it (invariant #1).
            self.toplevel_consts.contains(name) || stdlib_declares(&self.requires, nesting, name)
        }
    }
    /// Does `simple` name a constant on `id` or any of its ancestors —
    /// either a `CONST = ...` assignment or a nested class/module? MRO
    /// incompleteness is irrelevant: only a positive hit is used, and it
    /// only ever silences a warning.
    fn const_in_ancestors(&self, id: ClassId, simple: &str) -> bool {
        let (chain, _complete) = self.ancestors(id);
        chain.into_iter().any(|a| {
            let c = self.class(a);
            c.consts.iter().any(|k| k == simple)
                || self.by_path.contains_key(&format!("{}::{simple}", c.path))
        })
    }

    /// (external-ancestry start namespaces, simple name) for a failed
    /// constant lookup (W3) — mirrors `const_exists`'s own split: a
    /// qualified name starts from its prefix's class chain, a bare name
    /// from every lexical scope class. The starts are where external
    /// knowledge (Tapioca RBI) has to take over, i.e. exactly
    /// `external_ancestor_starts`' two populations: ancestor names the
    /// project never resolved, AND ancestors that resolved onto a curated
    /// force-open declaration.
    ///
    /// The second population is load-bearing, measured (bead ita-dpg.1):
    /// while only `unresolved_ancestors` was consulted here, `Types::Base
    /// < GraphQL::Schema::Object` reached the gem RBI only because that
    /// superclass name resolved NOWHERE. Declaring the namespace in
    /// `declarations/rbs_collection.rbi` made it resolve, so the RBI walk
    /// never started and 2567 corpus-c E0104 warnings came back
    /// (`ID`/`Boolean`/`Int`, graphql-ruby's mixin names) — a declaration
    /// meant to REMOVE warnings adding them instead.
    fn external_lookup_starts<'a>(&self, nesting: &[String], name: &'a str) -> (Vec<String>, &'a str) {
        if let Some((prefix, simple)) = name.rsplit_once("::") {
            match self.resolve_const(nesting, prefix.trim_start_matches("::")) {
                Some(owner) => (self.external_ancestor_starts(owner), simple),
                None => (Vec::new(), simple),
            }
        } else {
            let mut starts = Vec::new();
            for level in nesting.iter().rev() {
                if let Some(&id) = self.by_path.get(level) {
                    starts.extend(self.external_ancestor_starts(id));
                }
            }
            (starts, name)
        }
    }

    /// Names written in `id`'s ancestor chain that the project's own
    /// index could not resolve — the external-ancestry entry points
    /// (W3). Mirrors `linearize`'s walk (same per-class resolution rule,
    /// same cycle guard), collecting the FAILED superclass / include /
    /// prepend names instead of the resolved ids.
    fn unresolved_ancestors(&self, id: ClassId) -> Vec<String> {
        let mut out = Vec::new();
        let mut visited = std::collections::HashSet::new();
        self.collect_unresolved(id, &mut out, &mut visited);
        out
    }

    fn collect_unresolved(
        &self,
        id: ClassId,
        out: &mut Vec<String>,
        visited: &mut std::collections::HashSet<ClassId>,
    ) {
        if !visited.insert(id) {
            return;
        }
        let class = self.class(id);
        let nesting = &class.nesting;
        for name in class.prepends.iter().chain(class.includes.iter()) {
            match self.resolve_const(nesting, name) {
                Some(m) => self.collect_unresolved(m, out, visited),
                None => out.push(name.trim_start_matches("::").to_string()),
            }
        }
        if let Some(sc) = &class.superclass {
            match self.resolve_superclass_const(id, nesting, sc) {
                Some(s) => self.collect_unresolved(s, out, visited),
                None => out.push(sc.trim_start_matches("::").to_string()),
            }
        }
    }

    /// Start namespaces where EXTERNAL knowledge (Tapioca RBI) has to
    /// take over for `id`'s own ancestry (bead ita-xze) — two distinct
    /// populations, both measured as real share of the `ancestry open`
    /// census bucket (45.7% and 40.2% respectively at the reference
    /// corpus), so a walk that only covered one would leave most of the
    /// bucket exactly as blind as before:
    ///
    /// 1. Names in `id`'s chain that never resolved in the project's own
    ///    index at all (`unresolved_ancestors` — same population
    ///    `external_lookup_starts` already walks for constant lookup: a
    ///    superclass/mixin spelled in project code but never defined
    ///    anywhere the project's own index reaches).
    /// 2. Ancestors that DID resolve but landed on a
    ///    `declarations/gems.rbi` force-open entry
    ///    (`OpenReason::DeclaredExternal`, see `merge_declared_fragment`):
    ///    the name is real and in the index, but that entry carries zero
    ///    methods by contract — only the real gem (Tapioca RBI) has them.
    ///    `unresolved_ancestors` cannot see these at all: they are not a
    ///    resolution failure, they resolve to a deliberately empty stub.
    ///    The start name for this population is the resolved ancestor's
    ///    own `path` (the RBI's fully-qualified header), not a name
    ///    written in project code.
    ///
    /// Walks `id`'s own resolved chain (`ancestors`, which includes `id`
    /// itself) for population 2 rather than reusing `inconclusive_reason`'s
    /// classification: that function reports only the FIRST project-side
    /// reason per call (a census aggregate), while every `DeclaredExternal`
    /// ancestor in the chain is a genuine, independent start point here.
    fn external_ancestor_starts(&self, id: ClassId) -> Vec<String> {
        let mut starts = self.unresolved_ancestors(id);
        let (chain, _complete) = self.ancestors(id);
        for a in chain {
            let class = self.class(a);
            if class.open_reason == Some(OpenReason::DeclaredExternal) {
                starts.push(class.path.clone());
            }
        }
        starts
    }

    /// Start namespaces for the Tapioca DSL-RBI method lookup
    /// (`dsl_method_lookup`, bead ita-tjr): `id`'s own `path`, then every
    /// PROJECT ancestor's `path` in MRO order (`ancestors`, which already
    /// includes `id` itself first). Deliberately the opposite population
    /// from `external_ancestor_starts`: a DSL RBI reopens the app's OWN
    /// class, under the exact name the project's own index already
    /// resolves it by — an ancestor this project could not resolve at
    /// all has no `path` of its own to look a DSL file up by (that's
    /// `unresolved_ancestors`' population, already covered by
    /// `external_ancestor_starts`), and a `DeclaredExternal` ancestor
    /// (`declarations/gems.rbi`) is external by construction. No
    /// filtering beyond that: `rbi_method_closure`'s own `rbi_map` lookup
    /// (case (c) of `resolve_method_node`) is what turns a name with no
    /// matching DSL file into a cheap, harmless miss.
    fn project_ancestor_starts(&self, id: ClassId) -> Vec<String> {
        let (chain, _complete) = self.ancestors(id);
        chain.into_iter().map(|a| self.class(a).path.clone()).collect()
    }

    /// Project classes that CONCLUSIVELY define `method` as an instance
    /// method (bead ita-dqo): their own fragment declares it
    /// (`methods_by_name`), AND their full ancestor chain — every class
    /// in it, including themselves — is both fully resolved
    /// (`ancestors().1`) and never `open`. An unresolved superclass/mixin
    /// name, or an `open` class anywhere in the chain (dynamic
    /// metaprogramming, `method_missing`, a curated external
    /// declaration), means some other, unknown definition could exist
    /// too — not a trustworthy positive proof for constraint
    /// contradiction (invariant #1: only PROVEN facts may narrow).
    /// Bounded by `methods_by_name[method].len()`, never a project-wide
    /// scan (bead ita-9p9's standing lesson) — the reverse index already
    /// narrowed to the classes that could possibly qualify.
    pub fn closed_candidates_for(&self, method: &str) -> Vec<ClassId> {
        let Some(ids) = self.methods_by_name.get(method) else {
            return Vec::new();
        };
        ids.iter()
            .copied()
            .filter(|&id| {
                let (chain, complete) = self.ancestors(id);
                complete && chain.iter().all(|&a| !self.class(a).open)
            })
            .collect()
    }
}

// -- W3 require/autoload: stdlib constants gated on the project's own
// `require` calls -----------------------------------------------

/// `constant path -> every stdlib lib whose `require` defines it`, parsed
/// once per process from the mechanically harvested inventory embedded at
/// compile time (`declarations/stdlib_constants.txt`, generator versioned
/// at `scripts/gen-stdlib-inventory.rb` — the anti-gaming rule's
/// generated-content exception, same pattern as `core_inventory.txt`).
fn stdlib_const_libs() -> &'static HashMap<&'static str, Vec<&'static str>> {
    use std::sync::LazyLock;
    static MAP: LazyLock<HashMap<&'static str, Vec<&'static str>>> = LazyLock::new(|| {
        const TXT: &str = include_str!("../declarations/stdlib_constants.txt");
        let mut map: HashMap<&'static str, Vec<&'static str>> = HashMap::new();
        for line in TXT.lines() {
            if line.starts_with('#') {
                continue;
            }
            if let Some((lib, path)) = line.split_once('\t') {
                map.entry(path).or_default().push(lib);
            }
        }
        map
    });
    &MAP
}

/// Singleton-track family (e): every `<Namespace>.<method>` pair the
/// stdlib really answers, parsed once per process from the mechanically
/// harvested inventory embedded at compile time
/// (`declarations/stdlib_singletons.txt`, generator versioned at
/// `scripts/gen-stdlib-singleton-inventory.rb` — the anti-gaming rule's
/// generated-content exception, same pattern as `core_inventory.txt`).
///
/// A SET of whole pairs, not a namespace->names map: the question asked
/// of it is always "does this exact namespace answer this exact name?",
/// and answering it with one hash of a borrowed line keeps the hot path
/// allocation-free for every miss.
fn stdlib_singleton_pairs() -> &'static std::collections::HashSet<&'static str> {
    use std::sync::LazyLock;
    static SET: LazyLock<std::collections::HashSet<&'static str>> = LazyLock::new(|| {
        const TXT: &str = include_str!("../declarations/stdlib_singletons.txt");
        TXT.lines().filter(|l| !l.starts_with('#') && !l.is_empty()).collect()
    });
    &SET
}

/// Does the stdlib itself answer `path.name` on its CLASS OBJECT?
///
/// Suppression only, and deliberately ungated on `require`: a hit turns
/// a would-be singleton `NotFound` into `Inconclusive`, so the worst it
/// can do is stay silent about a real typo on a stdlib namespace the
/// project never required. Gating it would be the only direction that
/// could produce a diagnostic on code that runs (invariant #1).
///
/// `::` prefixes are trimmed so `::FileUtils.mkdir_p` and
/// `FileUtils.mkdir_p` answer alike — the checker stores class paths
/// both ways depending on how the source wrote them.
pub fn stdlib_singleton_method(path: &str, name: &str) -> bool {
    let path = path.trim_start_matches("::");
    // One allocation per CANDIDATE, never per miss on an unrelated
    // namespace: the namespace check short-circuits first.
    if !stdlib_singleton_namespaces().contains(path) {
        return false;
    }
    stdlib_singleton_pairs().contains(format!("{path}.{name}").as_str())
}

/// Every namespace `stdlib_singletons.txt` says anything about. Lets
/// `stdlib_singleton_method` reject the overwhelming majority of
/// receivers (a project's own classes) before formatting a lookup key.
fn stdlib_singleton_namespaces() -> &'static std::collections::HashSet<&'static str> {
    use std::sync::LazyLock;
    static SET: LazyLock<std::collections::HashSet<&'static str>> = LazyLock::new(|| {
        stdlib_singleton_pairs()
            .iter()
            .filter_map(|pair| pair.rsplit_once('.').map(|(ns, _)| ns))
            .collect()
    });
    &SET
}

/// Every path `declarations/gems.rbi` declares (bead ita-3gs), as a
/// lookup set for `is_known_external_class_path`. Deliberately a raw
/// line scan of the embedded text, NOT `declarations::declared_fragments`
/// (which parses via the real `DefWalker`): `DefWalker`'s own `ClassNode`
/// arm calls `is_known_external_class_path` (bead ita-h6l) for every
/// class it walks, so parsing `gems.rbi` through it here would recurse
/// into THIS SAME `LazyLock` while it is still being computed — a
/// deadlock, not just wasted work. `gems.rbi`'s contract (enforced by
/// `declarations::tests::declares_open_namespaces_only_no_methods`) is
/// one bare `class`/`module <Path>; end` per line — a substring scan
/// buys nothing a real parse would add.
fn declared_gem_paths() -> &'static std::collections::HashSet<&'static str> {
    use std::sync::LazyLock;
    static SET: LazyLock<std::collections::HashSet<&'static str>> = LazyLock::new(|| {
        crate::declarations::GEMS_RBI
            .lines()
            .filter_map(|line| {
                let line = line.trim();
                let rest = line
                    .strip_prefix("class ")
                    .or_else(|| line.strip_prefix("module "))?;
                rest.split(';').next().map(str::trim)
            })
            .collect()
    });
    &SET
}

/// Bead ita-h6l (mechanism B): does `path` name a Ruby core class, a
/// stdlib constant this checker's inventory knows, or a curated gem
/// namespace? `class <path> ... end` written in project source for one of
/// these is Ruby's REOPENING syntax — the real class's ancestry and
/// method surface extend far past what this checker ever modeled
/// (`Pathname#exist?`, `Range#overlap?`, `Mail::Message#...`) — never the
/// declaration of a brand-new, closed project class. Detection is by path
/// only, independent of `closed_world`/`requires`: a project that spells
/// `class Pathname` unambiguously means the stdlib class, Gemfile or not.
/// Exact match only (no prefix/suffix): `Foo::Pathname` is a project
/// namespace, not a reopening, even though `Pathname` alone is not.
fn is_known_external_class_path(path: &str) -> bool {
    crate::core::is_known_core_constant(path)
        || stdlib_const_libs().contains_key(path)
        || declared_gem_paths().contains(path)
}

/// Does `name`, referenced from lexical `scope`, name a stdlib constant
/// whose defining lib this project actually requires (W3)? Consulted only
/// from `const_exists`, only after every project-side check failed — a
/// hit suppresses E0104 and nothing else (declaration, open ancestry, the
/// exact contract `declarations/gems.rbi` set). Without the matching
/// `require` in the project the constant keeps warning: gem-bundled
/// transitive requires are invisible here, and guessing them would turn
/// the gate into a blanket suppressor.
///
/// Exact paths only, never a truncated prefix: suppressing `Foo::Bar`
/// because `Foo` is stdlib-defined would hide a genuinely missing `Bar`.
/// Two candidate spellings, mirroring Ruby's cref lookup: the name as
/// written, and — for a bare name — the cref-qualified form, so a bare
/// `ParserError` inside an app-reopened `module JSON` still resolves to
/// `JSON::ParserError`.
pub fn stdlib_declares<S: std::hash::BuildHasher>(
    requires: &std::collections::HashSet<String, S>,
    nesting: &[String],
    name: &str,
) -> bool {
    if requires.is_empty() {
        return false;
    }
    let map = stdlib_const_libs();
    let name = name.trim_start_matches("::");
    // Every cref spelling, innermost out — `ParserError` inside
    // `module JSON; class X` resolves as `JSON::ParserError` through the
    // real lexical nesting (bead ita-519: never a string-truncation walk
    // over a flat scope — see `resolve_const`'s doc comment).
    if map
        .get(name)
        .is_some_and(|libs| libs.iter().any(|lib| requires.contains(&**lib)))
    {
        return true;
    }
    for level in nesting.iter().rev() {
        let cand = format!("{level}::{name}");
        if map
            .get(cand.as_str())
            .is_some_and(|libs| libs.iter().any(|lib| requires.contains(&**lib)))
        {
            return true;
        }
    }
    false
}

/// Do two `super_lookup_module` consumer outcomes agree well enough to
/// keep resolving `super` for a module shared by more than one consuming
/// class? No prior outcome always agrees; two `Found`s must name the exact
/// same method definition; two `NotFound`s agree with each other; anything
/// else is a genuine disagreement between consumers, which `super_lookup`
/// must treat as unresolvable rather than pick a side.
fn outcomes_agree(
    prior: Option<Result<(&MethodSig, ClassId), ()>>,
    here: Result<(&MethodSig, ClassId), ()>,
) -> bool {
    match (prior, here) {
        (None, _) => true,
        (Some(Ok((pm, _))), Ok((m, _))) => (pm.file, pm.name_span) == (m.file, m.name_span),
        (Some(Err(())), Err(())) => true,
        _ => false,
    }
}


#[cfg(test)]
mod dynamic_def_tests {
    use super::*;

    /// Visits every call node of the probed def body and records whether
    /// the TARGET name produced a reason.
    struct ReasonScan {
        name: &'static str,
        seen_reason: bool,
    }

    impl<'pr> ruby_prism::Visit<'pr> for ReasonScan {
        fn visit_call_node(&mut self, node: &ruby_prism::CallNode<'pr>) {
            if node.name().as_slice() == self.name.as_bytes() {
                assert!(
                    body_def_reason(node).is_some(),
                    "`{}` was expected to react in `body_def_reason` but \
                     returned None — the table in \
                     dynamic_def_prefilter_covers_every_reacting_name is \
                     stale",
                    self.name
                );
                self.seen_reason = true;
            }
            ruby_prism::visit_call_node(self, node);
        }
    }

    /// The prefilter and the reason must agree, name by name: a call
    /// name `body_def_reason` reacts to but that is missing from
    /// `BODY_DEF_NAMES` means the substring scan drops the whole body
    /// before the AST walk can see the call — the class stays CLOSED
    /// with its surface provably dynamic, the exact missed finding the
    /// prefilter exists to prevent (measured twice: the first version
    /// omitted `instance_eval` and 10 discourse sites came back
    /// unopened; the 2026-09-18 review added the `_exec` family). The
    /// table below is the spec of every name `body_def_reason` reacts
    /// to, spelled in the shape that makes it react. A new match arm
    /// without a prefilter entry fails the COVERED half here, and a
    /// stale table entry fails the REACTING half.
    #[test]
    fn dynamic_def_prefilter_covers_every_reacting_name() {
        // (name, the call source that makes `body_def_reason` react)
        let reacting: &[(&str, &str)] = &[
            ("class_eval", "class_eval(:X)"),
            ("module_eval", "module_eval(:X)"),
            ("instance_eval", "instance_eval(:X)"),
            ("instance_exec", "instance_exec(:X)"),
            ("class_exec", "class_exec(:X)"),
            ("module_exec", "module_exec(:X)"),
            ("define_method", "define_method(name)"),
            ("define_singleton_method", "define_singleton_method(name)"),
            ("alias_method", "alias_method(name, :other)"),
            ("attr_reader", "attr_reader(name)"),
            ("attr_writer", "attr_writer(name)"),
            ("attr_accessor", "attr_accessor(name)"),
        ];
        for (name, call_src) in reacting {
            // COVERED: the substring prefilter must let the body through.
            assert!(
                BODY_DEF_NAMES.contains(name),
                "`{name}` reacts in `body_def_reason` but is missing from \
                 BODY_DEF_NAMES — a def body containing only it is never \
                 walked and its openness is missed"
            );
            // REACTING: the parsed call really produces a reason, so the
            // table above stays honest about what "reacts" means.
            let src = format!("def __probe\n  {call_src}\nend");
            let parse = ruby_prism::parse(src.as_bytes());
            let mut scan = ReasonScan { name, seen_reason: false };
            ruby_prism::Visit::visit(&mut scan, &parse.node());
            assert!(
                scan.seen_reason,
                "`{call_src}` did not parse into a reachable call node"
            );
        }
    }

    /// The `send`/`public_send`/`__send__` family reacts through the
    /// one-level unwrap, and — unlike every name in the table above —
    /// it deliberately has NO `BODY_DEF_NAMES` entry of its own. It
    /// needs none: the literal form spells the inner definer in the
    /// body text, which is exactly what the substring prefilter scans,
    /// and giving `send` an entry would walk every method body that
    /// dispatches dynamically for no finding. That argument is a
    /// dependency between two mechanisms, so it is asserted here
    /// instead of narrated: each unwrapped shape must both pass the
    /// prefilter and really produce a reason.
    #[test]
    fn the_send_form_passes_the_prefilter() {
        for send in ["send", "public_send", "__send__"] {
            assert!(
                !BODY_DEF_NAMES.contains(&send),
                "`{send}` must stay out of the prefilter: the inner name carries it"
            );
            for definer in [
                "define_method",
                "define_singleton_method",
                "alias_method",
                "attr_accessor",
            ] {
                let call_src = format!("{send}(:{definer}, name)");
                assert!(
                    BODY_DEF_NAMES.iter().any(|n| call_src.contains(n)),
                    "`{call_src}` must pass the substring prefilter through its inner name"
                );
                let src = format!("def __probe\n  {call_src}\nend");
                let parse = ruby_prism::parse(src.as_bytes());
                let mut scan = ReasonScan { name: send, seen_reason: false };
                ruby_prism::Visit::visit(&mut scan, &parse.node());
                assert!(scan.seen_reason, "`{call_src}` did not react");
            }
        }
    }
}
