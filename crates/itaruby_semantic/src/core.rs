//! Static core-library method table: a curated allowlist of well-known
//! `Integer`/`Float`/`String`/`Symbol`/`Array`/`Hash`/`NilClass`/`Bool`/
//! `Object` methods used for arity checking and return-type inference.
//!
//! This is deliberately incomplete. The full Ruby core library cannot be
//! enumerated here, so a method missing from this table is NOT treated as
//! "does not exist" — the checker resolves it to `Ty::Unknown` and emits no
//! diagnostic (invariant #1: `Unknown` never produces a diagnostic).
//! `unknown-method` (E0101) is only ever raised against project classes
//! that resolved to a specific, non-open `ClassId`.

use crate::types::Ty;

/// A core class this table has entries for.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CoreClass {
    Integer,
    Float,
    Str,
    Sym,
    Array,
    Hash,
    Nil,
    Bool,
    /// `Object`/`Kernel` methods, available on every receiver as a
    /// fallback. `core_method` does NOT apply this fallback itself — the
    /// checker decides whether to retry a miss against `Object`.
    Object,
}

/// Symbolic return type of a core method call. Resolving these against a
/// concrete receiver `Ty` (e.g. `SelfSame` -> the receiver's own type,
/// `Elem` -> an `Array`'s element type) is the checker's job, not this
/// table's.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CoreRet {
    Int,
    Float,
    Str,
    Sym,
    Bool,
    Nil,
    /// Same type as the receiver (e.g. `Integer#abs`, `String#strip`,
    /// `Array#sort`).
    SelfSame,
    /// Element type of an `Array` receiver (`Array#first`, `#last`, `#pop`).
    Elem,
    /// `Array[K]` of a `Hash` receiver (`Hash#keys`).
    KeyArray,
    /// `Array[V]` of a `Hash` receiver (`Hash#values`).
    ValArray,
    /// `Array[Str]` (`String#split`, `String#chars`).
    StrArray,
    Unknown,
}

/// One core method's arity and return shape. `max_args: None` means
/// varargs/rest (no upper bound).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct CoreMethod {
    pub min_args: u8,
    pub max_args: Option<u8>,
    pub ret: CoreRet,
}

const fn m(min_args: u8, max_args: Option<u8>, ret: CoreRet) -> CoreMethod {
    CoreMethod {
        min_args,
        max_args,
        ret,
    }
}

/// Looks up `name` on core class `class`. `None` means "not in the
/// allowlist" — the caller must treat that as `Ty::Unknown`, not as
/// nonexistent.
pub fn core_method(class: CoreClass, name: &str) -> Option<CoreMethod> {
    match class {
        CoreClass::Integer => integer_method(name),
        CoreClass::Float => float_method(name),
        CoreClass::Str => str_method(name),
        CoreClass::Sym => sym_method(name),
        CoreClass::Array => array_method(name),
        CoreClass::Hash => hash_method(name),
        CoreClass::Nil => nil_method(name),
        CoreClass::Bool => bool_method(name),
        CoreClass::Object => object_method(name),
    }
}

fn integer_method(name: &str) -> Option<CoreMethod> {
    use CoreRet::{Bool, Float, Int, SelfSame, Str, Unknown};
    match name {
        "+" | "-" | "*" | "/" | "%" | "**" => Some(m(1, Some(1), Int)),
        "to_s" => Some(m(0, Some(1), Str)),
        "to_i" => Some(m(0, Some(0), Int)),
        "to_f" => Some(m(0, Some(0), Float)),
        "abs" => Some(m(0, Some(0), SelfSame)),
        "zero?" | "positive?" | "negative?" | "even?" | "odd?" => Some(m(0, Some(0), Bool)),
        "times" => Some(m(0, Some(0), Unknown)),
        "succ" | "pred" => Some(m(0, Some(0), SelfSame)),
        "clamp" => Some(m(1, Some(2), SelfSame)),
        "<=>" => Some(m(1, Some(1), Int)),
        "==" | "!=" | "<" | "<=" | ">" | ">=" => Some(m(1, Some(1), Bool)),
        _ => None,
    }
}

fn float_method(name: &str) -> Option<CoreMethod> {
    use CoreRet::{Bool, Float, Int, SelfSame, Str};
    match name {
        "+" | "-" | "*" | "/" | "%" | "**" => Some(m(1, Some(1), Float)),
        "to_i" => Some(m(0, Some(0), Int)),
        "to_s" => Some(m(0, Some(0), Str)),
        "round" | "ceil" | "floor" => Some(m(0, Some(1), Int)),
        "abs" => Some(m(0, Some(0), SelfSame)),
        "zero?" | "positive?" | "negative?" | "nan?" => Some(m(0, Some(0), Bool)),
        "<=>" => Some(m(1, Some(1), Int)),
        "==" | "!=" | "<" | "<=" | ">" | ">=" => Some(m(1, Some(1), Bool)),
        _ => None,
    }
}

fn str_method(name: &str) -> Option<CoreMethod> {
    use CoreRet::{Bool, Float, Int, SelfSame, Str, StrArray, Sym, Unknown};
    match name {
        "+" => Some(m(1, Some(1), Str)),
        "length" | "size" => Some(m(0, Some(0), Int)),
        "upcase" | "downcase" | "capitalize" | "strip" | "lstrip" | "rstrip" | "chomp" | "chop"
        | "reverse" => Some(m(0, Some(0), SelfSame)),
        "empty?" => Some(m(0, Some(0), Bool)),
        "include?" | "start_with?" | "end_with?" => Some(m(1, None, Bool)),
        "split" => Some(m(0, Some(2), StrArray)),
        "gsub" | "sub" => Some(m(1, Some(2), Str)),
        "to_i" => Some(m(0, Some(1), Int)),
        "to_f" => Some(m(0, Some(0), Float)),
        "to_s" => Some(m(0, Some(0), SelfSame)),
        "to_sym" => Some(m(0, Some(0), Sym)),
        "[]" => Some(m(1, Some(2), Unknown)),
        "*" => Some(m(1, Some(1), Str)),
        "==" | "!=" => Some(m(1, Some(1), Bool)),
        "chars" => Some(m(0, Some(0), StrArray)),
        "freeze" => Some(m(0, Some(0), SelfSame)),
        _ => None,
    }
}

fn sym_method(name: &str) -> Option<CoreMethod> {
    use CoreRet::{Bool, Int, SelfSame, Str, Unknown};
    match name {
        "to_s" => Some(m(0, Some(0), Str)),
        "to_sym" => Some(m(0, Some(0), SelfSame)),
        "to_proc" => Some(m(0, Some(0), Unknown)),
        "length" | "size" => Some(m(0, Some(0), Int)),
        "==" => Some(m(1, Some(1), Bool)),
        _ => None,
    }
}

fn array_method(name: &str) -> Option<CoreMethod> {
    use CoreRet::{Bool, Elem, Int, SelfSame, Str, Unknown};
    match name {
        "<<" | "push" => Some(m(0, None, SelfSame)),
        "pop" | "shift" | "first" | "last" | "min" | "max" => Some(m(0, Some(1), Elem)),
        "map" | "collect" | "select" | "filter" | "reject" | "flat_map" => Some(m(0, Some(0), Unknown)),
        "each" | "each_with_index" => Some(m(0, Some(0), SelfSame)),
        "length" | "size" | "count" => Some(m(0, Some(1), Int)),
        "empty?" => Some(m(0, Some(0), Bool)),
        "include?" => Some(m(1, Some(1), Bool)),
        "join" => Some(m(0, Some(1), Str)),
        "sort" | "sort_by" | "uniq" | "flatten" | "compact" | "reverse" | "shuffle" => {
            Some(m(0, Some(1), SelfSame))
        }
        "sum" => Some(m(0, Some(1), Unknown)),
        "index" | "find_index" => Some(m(0, Some(1), Unknown)),
        // `concat(*other_arrays)` is varargs since Ruby 2.4 (even `concat()`
        // with zero args is valid — verified `ruby --disable-gems -e
        // 'a=[1]; a.concat; p a'` => `[1]`, and `a.concat([2],[3])` => `[1,2,3]`
        // on Ruby 3.4.2). `+`/`-` stay fixed at exactly 1 arg (binary
        // operators; `Array.instance_method(:+).arity` and `:-` both `1`).
        // Pinning `concat` to `(1, Some(1))` alongside `+`/`-` produced a
        // real false E0102 on `diagnostics.concat(a, b)` (bead ita-d4).
        "concat" => Some(m(0, None, SelfSame)),
        "+" | "-" => Some(m(1, Some(1), SelfSame)),
        "[]" => Some(m(1, Some(2), Unknown)),
        "==" => Some(m(1, Some(1), Bool)),
        "any?" | "all?" | "none?" => Some(m(0, Some(1), Bool)),
        "to_a" => Some(m(0, Some(0), SelfSame)),
        _ => None,
    }
}

fn hash_method(name: &str) -> Option<CoreMethod> {
    use CoreRet::{Bool, Int, KeyArray, SelfSame, Unknown, ValArray};
    match name {
        "[]" => Some(m(1, Some(1), Unknown)),
        "[]=" => Some(m(2, Some(2), Unknown)),
        "fetch" => Some(m(1, Some(2), Unknown)),
        "key?" | "has_key?" | "include?" | "member?" | "value?" | "has_value?" => {
            Some(m(1, Some(1), Bool))
        }
        "keys" => Some(m(0, Some(0), KeyArray)),
        "values" => Some(m(0, Some(0), ValArray)),
        "length" | "size" | "count" => Some(m(0, Some(0), Int)),
        "empty?" => Some(m(0, Some(0), Bool)),
        "merge" => Some(m(0, None, SelfSame)),
        "each" | "each_pair" => Some(m(0, Some(0), SelfSame)),
        "map" => Some(m(0, Some(0), Unknown)),
        "delete" => Some(m(1, Some(1), Unknown)),
        "to_h" => Some(m(0, Some(0), SelfSame)),
        "any?" => Some(m(0, Some(1), Bool)),
        _ => None,
    }
}

fn nil_method(name: &str) -> Option<CoreMethod> {
    use CoreRet::{Bool, Int, Str, Unknown};
    match name {
        "nil?" => Some(m(0, Some(0), Bool)),
        "to_s" | "inspect" => Some(m(0, Some(0), Str)),
        "to_a" => Some(m(0, Some(0), Unknown)),
        "to_i" => Some(m(0, Some(0), Int)),
        _ => None,
    }
}

fn bool_method(name: &str) -> Option<CoreMethod> {
    use CoreRet::{Bool, Str};
    match name {
        "!" => Some(m(0, Some(0), Bool)),
        "&" | "|" | "^" => Some(m(1, Some(1), Bool)),
        "to_s" | "inspect" => Some(m(0, Some(0), Str)),
        _ => None,
    }
}

fn object_method(name: &str) -> Option<CoreMethod> {
    use CoreRet::{Bool, Int, Nil, SelfSame, Str, Unknown};
    match name {
        "nil?" => Some(m(0, Some(0), Bool)),
        "block_given?" => Some(m(0, Some(0), Bool)),
        // Kernel conversion functions (capitalized method names).
        "Integer" | "String" | "Float" | "Array" | "Hash" | "Rational" | "Complex"
        | "BigDecimal" | "URI" | "Pathname" | "JSON" => Some(m(1, Some(2), Unknown)),
        "caller" | "binding" | "catch" | "throw" | "exit" | "exit!" | "abort" | "at_exit"
        | "gets" | "system" | "printf" | "pp" | "srand" | "open" | "load" | "autoload"
        | "instance_variable_defined?" | "instance_variables" | "methods"
        | "public_methods" | "private_methods" | "singleton_class" | "define_singleton_method"
        | "display" | "extend" | "instance_exec" | "instance_eval" | "method_missing"
        | "enum_for" | "to_enum" => Some(m(0, None, Unknown)),
        "is_a?" | "kind_of?" | "instance_of?" => Some(m(1, Some(1), Bool)),
        "respond_to?" => Some(m(1, Some(2), Bool)),
        "tap" | "then" | "yield_self" => Some(m(0, Some(0), Unknown)),
        "class" => Some(m(0, Some(0), Unknown)),
        "freeze" | "dup" | "clone" | "itself" => Some(m(0, Some(0), SelfSame)),
        "frozen?" => Some(m(0, Some(0), Bool)),
        "inspect" | "to_s" => Some(m(0, Some(0), Str)),
        "==" | "!=" | "equal?" | "eql?" => Some(m(1, Some(1), Bool)),
        "hash" => Some(m(0, Some(0), Int)),
        "object_id" => Some(m(0, Some(0), Int)),
        "send" | "public_send" | "__send__" => Some(m(1, None, Unknown)),
        "method" => Some(m(1, Some(1), Unknown)),
        "instance_variable_get" => Some(m(1, Some(1), Unknown)),
        "instance_variable_set" => Some(m(2, Some(2), Unknown)),
        // `pp` is deliberately absent: the arm above already claims it
        // (`m(0, None, Unknown)`), so listing it here was dead — and the
        // surviving one is the safer of the two under invariant #1.
        "puts" | "p" | "print" | "warn" => Some(m(0, None, Nil)),
        "raise" | "fail" => Some(m(0, Some(3), Unknown)),
        "require" | "require_relative" => Some(m(1, Some(1), Bool)),
        "loop" => Some(m(0, Some(0), Unknown)),
        "rand" | "sleep" => Some(m(0, Some(1), Unknown)),
        "format" | "sprintf" => Some(m(1, None, Str)),
        _ => None,
    }
}

/// Maps a concrete `Ty` to the core class whose method table applies to it.
/// Project types (`Instance`/`Class`), `Union`, and `Unknown` have no core
/// table — `None`.
pub fn core_class_of(ty: &Ty) -> Option<CoreClass> {
    match ty {
        Ty::Int => Some(CoreClass::Integer),
        Ty::Float => Some(CoreClass::Float),
        Ty::Str => Some(CoreClass::Str),
        Ty::Sym => Some(CoreClass::Sym),
        Ty::Array(_) => Some(CoreClass::Array),
        Ty::Hash(_, _) => Some(CoreClass::Hash),
        Ty::Nil => Some(CoreClass::Nil),
        Ty::Bool => Some(CoreClass::Bool),
        Ty::Instance(_) | Ty::Class(_) | Ty::Union(_) | Ty::Unknown => None,
    }
}

/// Does core method `name` run its block with LEXICAL `self`?
///
/// Ruby has exactly one family of ways to change `self` inside a block:
/// `instance_eval`/`instance_exec`/`class_eval`/`module_eval`/`class_exec`/
/// `module_exec`/`define_method`. Every method named below merely YIELDS,
/// so a receiverless call inside its block dispatches on the ENCLOSING
/// `self` with certainty — which is what lets `check_call` keep E0101
/// conclusive there (bead ita-uye) despite the blanket softening of bead
/// ita-4xy.
///
/// Deliberately an allowlist and not a denylist of the rebinding family: a
/// name absent here is merely unproven, and unproven costs a false
/// negative, never a false positive (invariant #1). The caller must ALSO
/// prove the receiver's core class was never reopened — a reopened
/// `Array#each` may rebind `self` no matter what this table says. See
/// `Checker::block_keeps_lexical_self`.
pub fn core_block_keeps_lexical_self(name: &str) -> bool {
    matches!(
        name,
        // Enumerable / Array / Hash iteration and folding.
        "each"
            | "each_with_index"
            | "each_with_object"
            | "each_entry"
            | "each_index"
            | "each_pair"
            | "each_key"
            | "each_value"
            | "each_slice"
            | "each_cons"
            | "each_char"
            | "each_line"
            | "each_byte"
            | "reverse_each"
            | "map"
            | "map!"
            | "collect"
            | "collect!"
            | "flat_map"
            | "filter_map"
            | "select"
            | "select!"
            | "filter"
            | "filter!"
            | "reject"
            | "reject!"
            | "find"
            | "detect"
            | "find_all"
            | "delete_if"
            | "keep_if"
            | "sort_by"
            | "sort_by!"
            | "min_by"
            | "max_by"
            | "group_by"
            | "partition"
            | "sum"
            | "count"
            | "reduce"
            | "inject"
            | "take_while"
            | "drop_while"
            | "transform_keys"
            | "transform_values"
            | "fetch"
            | "all?"
            | "any?"
            | "none?"
            | "one?"
            // Integer iteration.
            | "times"
            | "upto"
            | "downto"
            | "step"
            // Kernel/Object: these yield `self` as an ARGUMENT, they never
            // rebind it.
            | "tap"
            | "then"
            | "yield_self"
    )
}

/// The generated inventory (beads ita-2ve, ita-d2): one `Class#method`
/// line per public/protected instance method of each core class this
/// module models, plus one `::Name` line per top-level constant
/// (`Object.constants`) — harvested mechanically from the Ruby runtime by
/// `scripts/gen-core-inventory.rb` (see that file's header for the exact
/// regeneration command and the Ruby version used). Embedded at compile
/// time — same pattern as `declarations/gems.rbi`. Unlike the allowlist
/// above, this is the language's full surface, which is what lets
/// `check.rs`'s closed-world path conclude "does not exist" without
/// manufacturing false positives on real-but-unallowlisted methods
/// (`"s".center(10)`).
const CORE_INVENTORY: &str = include_str!("../declarations/core_inventory.txt");

/// Parsed `CORE_INVENTORY` (`Class#method` -> set). Built once per
/// process; header `#` lines and blanks are skipped.
static INVENTORY: std::sync::LazyLock<std::collections::HashSet<&'static str>> =
    std::sync::LazyLock::new(|| {
        CORE_INVENTORY
            .lines()
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .collect()
    });

/// Is `Class#method` in the generated inventory? Only consulted after
/// `core_method` already missed, so a `false` here means "not in the
/// allowlist AND not in the language's actual surface".
pub fn core_inventory_has(class: &str, method: &str) -> bool {
    INVENTORY.contains(format!("{class}#{method}").as_str())
}

/// Is `name` present as a harvested top-level constant in
/// `CORE_INVENTORY` (bead ita-d2 — `SystemStackError` was the proven
/// gap)? Distinct line format from `core_inventory_has`'s `Class#method`
/// (no `#` ever appears in a constant name): `::Name`, mirroring how
/// Ruby itself spells an absolute top-level constant reference.
/// Consulted from `is_known_core_constant` below, additively — a `false`
/// here just falls through to that function's hand-curated list, never
/// removes a name it already covers.
pub fn core_inventory_has_const(name: &str) -> bool {
    INVENTORY.contains(format!("::{name}").as_str())
}

/// bead ita-4xy follow-up, gap 2: `Kernel`'s own surface is almost
/// entirely `private` (`module_function` makes a method public on the
/// MODULE but private on every object that mixes it in — `proc`,
/// `lambda`, `format`, ...), which is why `CORE_INVENTORY` has zero
/// `Kernel#`/`BasicObject#` lines: `gen-core-inventory.rb` harvests
/// `Class#instance_methods`, and Ruby's own `instance_methods` returns
/// PUBLIC/PROTECTED only (verified against
/// `declarations/core_inventory.txt`, 2026-08-21). `Object`'s own
/// public/protected surface is NOT duplicated here — it is already the
/// `Object#` lines `core_inventory_has` reads. Regenerating the
/// inventory to cover this would mean teaching the generator to harvest
/// PRIVATE methods too, a `gen-core-inventory.rb` change out of this
/// bead's scope (never a lookup-layer decision) — hand-picked instead,
/// straight from `Kernel.private_instance_methods(false)` /
/// `BasicObject.instance_methods(false)` /
/// `BasicObject.private_instance_methods(false)` on Ruby 3.4.2, minus
/// every name `object_method`/`CORE_INVENTORY`'s `Object#` lines
/// already cover.
///
/// ponytail: hand-maintained, not generated — delete this list (and
/// `CLASS_MODULE_ONLY_METHODS` below) once `gen-core-inventory.rb`
/// learns to harvest `Kernel`/`BasicObject`/`Class`/`Module` (public AND
/// private) as inventory classes of their own.
///
/// Deliberately EXCLUDES `initialize`, even though it is a genuine
/// `BasicObject` private method: `check.rs`'s `Ty::Class` + `name ==
/// "new"` arm queries `lookup_method_rbi(c, "initialize", ...)`
/// internally to arity-check `.new` against an implicit 0-arg
/// constructor when the class defines none. Treating `initialize` as a
/// blanket Kernel/Object hit would silence THAT query too, turning
/// every `SomeClass.new(unexpected, args)` on a class with no explicit
/// `initialize` from a checked `E0102` into permanent `Inconclusive` —
/// a real regression unrelated to either of this bead's two gaps.
///
/// `gem` is the one entry here NOT from `Kernel.private_instance_methods`
/// on a bare `ruby` invocation (bead ita-o8l.3, corpus site
/// `actionpack/.../system_testing/driver.rb:18`,
/// `gem "selenium-webdriver", ">= 4.0.0"`): it is added by `RubyGems`, not
/// the Ruby language proper, and is therefore invisible to
/// `gen-core-inventory.rb`, which deliberately runs `ruby
/// --disable-gems` (see that script's header — the flag guarantees the
/// harvest reflects the LANGUAGE, not the local bundle, and is load-
/// bearing for closed-world correctness elsewhere; widening the harvest
/// to re-enable `RubyGems` was rejected specifically to avoid reopening
/// that guarantee for one name). `RubyGems` itself is part of every
/// default Ruby install/bootstrap since 1.9 and is off only under the
/// generator's own deliberate `--disable-gems` flag — a real
/// application's own runtime always has `RubyGems` loaded, making `gem`
/// functionally as universal as any other name in this list. Hand-added
/// here, same table, same fallback path, in preference to touching the
/// generator.
const KERNEL_PRIVATE_INSTANCE_METHODS: &[&str] = &[
    "__callee__", "__dir__", "__method__", "`", "autoload?", "caller_locations",
    "eval", "exec", "fork", "gem", "global_variables", "initialize_clone",
    "initialize_copy", "initialize_dup", "iterator?", "lambda", "local_variables", "proc",
    "putc", "readline", "readlines", "respond_to_missing?", "select", "set_trace_func",
    "singleton_method_added", "singleton_method_removed", "singleton_method_undefined", "spawn", "syscall", "test",
    "trace_var", "trap", "untrace_var",
];

/// Kernel/top-level DSL methods contributed by PUBLIC gems, never by app
/// code (bead ita-yxc, round-5 audit of the four public corpora,
/// 2026-08-25): same conceptual category as
/// `KERNEL_PRIVATE_INSTANCE_METHODS` above — a bare (implicit-receiver)
/// call resolves to a real method no project class ever defines, so
/// treating it as `NotFound` is a false E0101, never a true one. Unlike
/// Ruby's OWN Kernel surface, these come from a gem mixing itself into
/// `Object`/`Kernel` at runtime, which is exactly why they can never be
/// enumerated by `gen-core-inventory.rb` (no local Ruby install has these
/// gems loaded) and must be hand-curated here instead, one gem call
/// syntax at a time.
///
/// ENTRY RULE (same discipline as `declarations/gems.rbi`): the name must
/// have appeared in the round-5 measurement as a bare top-level call this
/// checker misresolved, and the gem contributing it must be public. So
/// far: `Stoplight` (`stoplight` gem, `Stoplight(name) { ... }` circuit
/// breaker DSL — 2 sites, mastodon), `Rainbow` (`rainbow` gem,
/// `Rainbow(text).color` — 16 sites, gitlab-foss), `_`/`s_`
/// (`fast_gettext` gem's `FastGettext::Translation`, mixed into `Object`
/// — `_('text')`/`s_('scoped')` — 31 sites, gitlab-foss).
///
/// `_`/`s_` are single/two-char names that collide syntactically with the
/// common "discard" local-variable convention (`x, _ = pair`) — safe
/// here regardless, because this table is only ever consulted from
/// `ProjectIndex::soften_not_found`, itself only reached from
/// `lookup_method`/`lookup_singleton`'s method-CALL resolution
/// (`index.rs`). Prism already resolves a bare `_`/`s_` token to a
/// `LocalVariableReadNode`, never a `CallNode`, the moment either name is
/// assigned anywhere earlier in the same lexical scope — so a real
/// discard variable never reaches this table at all, and adding the name
/// here can only silence a genuine unresolved CALL, never shadow a
/// variable read.
const GEM_KERNEL_METHODS: &[&str] = &["Stoplight", "Rainbow", "_", "s_"];
/// Instance methods Mocha (`stubs`/`expects`/`unstub`, loaded via
/// `mocha/minitest`) and Minitest's own `minitest/mock` (`stub`) mix into
/// `Object` when the test framework is required — never a genuine Ruby
/// core method, but wired through the exact same "Object gained a method
/// no generated inventory can enumerate" gap as
/// `KERNEL_PRIVATE_INSTANCE_METHODS` (bead ita-se5, Round-5 measurement:
/// 11 discourse sites on Mocha `stubs`/`expects`, e.g.
/// `spec/lib/onebox/matcher_spec.rb:61`; 4 rails sites on Minitest's
/// `stub`, e.g. `actionpack/test/dispatch/debug_exceptions_test.rb:586`
/// `backtrace_cleaner.stub :clean, [...] do ... end` — the CALL to
/// `.stub` itself, not the temporarily-stubbed method name used inside
/// the block, which would need interprocedural tracking and is out of
/// this bead's scope). All 15/15 measured sites are inside `spec/`/
/// `test/` files, but nothing here threads a "is this file a test file"
/// signal down to the lookup that consults this list, so this is
/// deliberately unscoped by path: mocha/minitest are conditionally
/// loaded (require'd only under a test runner), so a stray `.stubs`/
/// `.stub` call outside test code without the gem loaded would raise a
/// real `NoMethodError` this table now silences too — an accepted false
/// negative (invariant #1 only forbids false positives), and the
/// pragmatic ponytail call given path-scoping would suppress the exact
/// same 15 sites for meaningfully more machinery.
const TEST_FRAMEWORK_OBJECT_MIXIN_METHODS: &[&str] = &["stubs", "expects", "unstub", "stub"];

/// `Object`/`Kernel` `core_ext` methods `ActiveSupport` (Rails' own PUBLIC
/// support gem, `require "active_support/all"` or any of its narrower
/// requires) mixes into `Object` at runtime — never a genuine Ruby core
/// method, same "Object gained a method no generated inventory can
/// enumerate" gap as `GEM_KERNEL_METHODS`/
/// `TEST_FRAMEWORK_OBJECT_MIXIN_METHODS` above (bead ita-o8l.2,
/// precision-report cluster §7.2, 2026-08-25: 14 confirmed sites, 13
/// rails + 1 discourse, all read individually). **Not** solved through
/// the generated core-inventory pack: activesupport's own `.rbs`
/// reopens `Object`, and `gen-core-inventory.rb`'s `core_stdlib_tops`
/// filter exists specifically to keep a core-namespace reopening OUT of
/// the pack (wave 9 defect 1 force-opened 41 core namespaces this way
/// and killed conclusive `E0101` project-wide) — declaring these four
/// names through the pack would reintroduce that exact defect, so they
/// are hand-curated here instead, in the one place that already exists
/// for precisely this shape.
///
/// ENTRY RULE (same discipline as `GEM_KERNEL_METHODS`): the name must
/// have appeared in the round-5/precision-report measurement as a
/// resolved-`Object`-ancestry call this checker misresolved, and the
/// gem contributing it must be public. So far: `to_json` (`require
/// "json"` + activesupport's `Object#to_json`/`as_json` — e.g.
/// `actionpack/.../journey/gtg/transition_table.rb:143`,
/// `activesupport/test/.../encoding_test.rb:199` — 6 sites), `try`
/// (`activemodel/test/.../error_test.rb:31` — 1 site), `instance_values`
/// (`activemodel/test/.../serialization_test.rb:40` — 1 site),
/// `acts_like?` (`activesupport/test/.../acts_like_test.rb:45,46,56,57`
/// — 4 sites), all four via `activesupport/lib/active_support/core_ext/
/// object`. Every measured site calls the bare name (implicit `self`)
/// or an explicit receiver whose OWN class never defines it, so
/// resolution genuinely falls through to `Object` — none of the four
/// names is a project-defined method anywhere in the four public
/// corpora measured.
const ACTIVE_SUPPORT_OBJECT_MIXIN_METHODS: &[&str] =
    &["to_json", "try", "instance_values", "acts_like?"];

/// Is `name` a Kernel/Object/BasicObject INSTANCE method every Ruby
/// object responds to, closed project class or not (gap 2)? `Object`'s
/// public/protected surface via `core_inventory_has`, Kernel's private
/// surface (`proc`, `lambda`, ...) via `KERNEL_PRIVATE_INSTANCE_METHODS`
/// above, public-gem Kernel-level DSL (`Stoplight`, `Rainbow`, `_`, ...)
/// via `GEM_KERNEL_METHODS`, Mocha/Minitest's test-framework mixins via
/// `TEST_FRAMEWORK_OBJECT_MIXIN_METHODS`, and `ActiveSupport`'s own
/// `Object` `core_ext` via `ACTIVE_SUPPORT_OBJECT_MIXIN_METHODS` (beads
/// ita-yxc, ita-se5, ita-o8l.2).
pub fn kernel_object_instance_method(name: &str) -> bool {
    core_inventory_has("Object", name)
        || KERNEL_PRIVATE_INSTANCE_METHODS.contains(&name)
        || GEM_KERNEL_METHODS.contains(&name)
        || TEST_FRAMEWORK_OBJECT_MIXIN_METHODS.contains(&name)
        || ACTIVE_SUPPORT_OBJECT_MIXIN_METHODS.contains(&name)
}

/// `Class`/`Module`'s own instance methods (`new`, `ancestors`,
/// `include`, `private`, ...) — see `KERNEL_PRIVATE_INSTANCE_METHODS`'s
/// doc comment for why these are hand-picked rather than harvested.
/// Ruby 3.4.2's `Class.instance_methods(false)` +
/// `Class.private_instance_methods(false)` + `Module.instance_methods(false)` +
/// `Module.private_instance_methods(false)`, minus every name already
/// covered by `kernel_object_instance_method`.
const CLASS_MODULE_ONLY_METHODS: &[&str] = &[
    "<", "<=", ">", ">=", "alias_method", "allocate",
    "ancestors", "append_features", "attached_object", "attr", "attr_accessor", "attr_reader",
    "attr_writer", "class_eval", "class_exec", "class_variable_defined?", "class_variable_get", "class_variable_set",
    "class_variables", "const_added", "const_defined?", "const_get", "const_missing", "const_set",
    "const_source_location", "constants", "define_method", "deprecate_constant", "extend_object", "extended",
    "include", "include?", "included", "included_modules", "inherited", "instance_method",
    "instance_methods", "method_added", "method_defined?", "method_removed", "method_undefined", "module_eval",
    "module_exec", "module_function", "name", "new", "prepend", "prepend_features",
    "prepended", "private", "private_class_method", "private_constant", "private_instance_methods", "private_method_defined?",
    "protected", "protected_instance_methods", "protected_method_defined?", "public", "public_class_method", "public_constant",
    "public_instance_method", "public_instance_methods", "public_method_defined?", "refine", "refinements", "remove_class_variable",
    "remove_const", "remove_method", "ruby2_keywords", "set_temporary_name", "singleton_class?", "subclasses",
    "superclass", "undef_method", "undefined_instance_methods", "using",
];

/// Is `name` a Class/Module/Object/Kernel/BasicObject method every Ruby
/// class OBJECT responds to (gap 2, SINGLETON side — `def self.x`
/// bodies)? A class object's own ancestry is `Class < Module < Object <
/// Kernel < BasicObject`, so it answers to everything
/// `kernel_object_instance_method` already covers, plus `Class`/
/// `Module`'s own surface (`CLASS_MODULE_ONLY_METHODS`).
pub fn kernel_object_singleton_method(name: &str) -> bool {
    kernel_object_instance_method(name) || CLASS_MODULE_ONLY_METHODS.contains(&name)
}

/// Inventory names for a core class — `Bool` is the only two-class case
/// (a conclusive miss must be absent from both).
pub fn core_inventory_names(class: CoreClass) -> &'static [&'static str] {
    match class {
        CoreClass::Integer => &["Integer"],
        CoreClass::Float => &["Float"],
        CoreClass::Str => &["String"],
        CoreClass::Sym => &["Symbol"],
        CoreClass::Array => &["Array"],
        CoreClass::Hash => &["Hash"],
        CoreClass::Nil => &["NilClass"],
        CoreClass::Bool => &["TrueClass", "FalseClass"],
        CoreClass::Object => &["Object"],
    }
}

/// Ruby names whose reopening (a project fragment — condition (b) of the
/// closed-world contract, checked via `ProjectIndex::by_path`) can add
/// instance methods to `class`'s receivers: the class itself plus the
/// ancestors/mixins its real method table runs through. `Object`/`Kernel`
/// affect every receiver.
pub fn core_pollution_names(class: CoreClass) -> &'static [&'static str] {
    match class {
        CoreClass::Integer => &["Integer", "Numeric", "Comparable", "Object", "Kernel"],
        CoreClass::Float => &["Float", "Numeric", "Comparable", "Object", "Kernel"],
        CoreClass::Str => &["String", "Comparable", "Object", "Kernel"],
        CoreClass::Sym => &["Symbol", "Comparable", "Object", "Kernel"],
        CoreClass::Array => &["Array", "Enumerable", "Object", "Kernel"],
        CoreClass::Hash => &["Hash", "Enumerable", "Object", "Kernel"],
        CoreClass::Nil => &["NilClass", "Object", "Kernel"],
        CoreClass::Bool => &["TrueClass", "FalseClass", "Object", "Kernel"],
        CoreClass::Object => &["Object", "Kernel"],
    }
}

/// The names whose definition on the RECEIVER's own class could make
/// `<literal> <op> <literal>` legal: exactly the operator.
///
/// Own class only, and that is a measurement, not an economy. On ruby
/// 3.4.2, with `p 1 + "s"`: `class Integer; def +(o) = "x"; end` and
/// `Integer.prepend(P)` where `P#+` exists both print and exit 0, while
/// `class Object; def +(o) = "x"; end`, `class Numeric; def +...` and
/// `module Comparable; def +...` ALL still raise `TypeError` — `Integer`
/// defines `+` itself, so nothing later in the chain is ever reached.
/// The same holds for `"a" + 1`: `String#+` wins over `Object#+`. A
/// `prepend`ed module's methods are folded onto the class's own key in
/// `index.rs`'s pollution maps, so this one name covers both shapes.
pub fn core_own_names(class: CoreClass) -> &'static [&'static str] {
    match class {
        CoreClass::Integer => &["Integer"],
        CoreClass::Float => &["Float"],
        CoreClass::Str => &["String"],
        CoreClass::Sym => &["Symbol"],
        CoreClass::Array => &["Array"],
        CoreClass::Hash => &["Hash"],
        CoreClass::Nil => &["NilClass"],
        CoreClass::Bool => &["TrueClass", "FalseClass"],
        CoreClass::Object => &["Object"],
    }
}

/// The names whose definition anywhere on the ARGUMENT's ancestry could
/// make the pairing legal — the conversion the operator itself performs,
/// plus the pair that answers for any missing name.
///
/// Measured on ruby 3.4.2 rather than read off the docs, because the
/// obvious guesses are wrong in both directions. For `p 1 + "s"`:
/// `String#coerce`, `Kernel#coerce` and `Object#coerce` each make it
/// print `2` and exit 0 (the numeric operators coerce the ARGUMENT, and
/// they find `coerce` anywhere in its chain), while `String#to_int` and
/// `Object#to_int` still raise — `to_int` is NOT on this path. For
/// `p "a" + 1` it is the other hook: `Object#to_str` makes it print
/// `"ao"`, and `String#+` is the receiver-side name. And
/// `Integer#method_missing` + `respond_to_missing?` makes `"a" + 1`
/// print `"amm"`, so the missing-method pair belongs here too.
pub fn arg_pollution_keys(recv: CoreClass) -> [&'static str; 3] {
    let hook = match recv {
        // `Integer#+`/`-`/`*`/`/` coerce the argument.
        CoreClass::Integer | CoreClass::Float => "coerce",
        // `String#+` converts the argument with `to_str`.
        CoreClass::Str => "to_str",
        // No other receiver class reaches E0108's emit (see
        // `check_operand_types`'s pairing table); `coerce` is the
        // fail-closed answer if one ever does.
        _ => "coerce",
    };
    [hook, "method_missing", "respond_to_missing?"]
}

/// Every name `is_core_namespace` answers yes to — the list itself, so a
/// pass that needs to visit the core classes a project reopened can look
/// each one up instead of testing every project class it has
/// (`resolve_fragment_pollution`: 14 hash lookups instead of one alias
/// chase per class in the repository, measured at 19% of
/// `check/project_index` before the inversion).
pub fn core_namespace_names() -> &'static [&'static str] {
    &[
        "Integer",
        "Float",
        "Numeric",
        "String",
        "Symbol",
        "Array",
        "Hash",
        "NilClass",
        "TrueClass",
        "FalseClass",
        "Object",
        "Kernel",
        "Comparable",
        "Enumerable",
    ]
}

/// Is `name` a core class/mixin whose monkeypatching could reach a core
/// receiver (the receiver set of `index.rs`'s method-injection detection)?
/// Deliberately narrower than `is_known_core_constant`: `ENV` or `Struct`
/// gaining methods says nothing about the nine receiver classes.
pub fn is_core_namespace(name: &str) -> bool {
    matches!(
        name,
        "Integer"
            | "Float"
            | "Numeric"
            | "String"
            | "Symbol"
            | "Array"
            | "Hash"
            | "NilClass"
            | "TrueClass"
            | "FalseClass"
            | "Object"
            | "Kernel"
            | "Comparable"
            | "Enumerable"
    )
}

/// The core `Ty` an `x.is_a?(<core class>)` predicate proves in its true
/// branch (bead ita-2ve). `Object` and everything outside the nine
/// receiver classes yield `None` — no fact, same as before (a var that is
/// an Object is any value; narrowing there would be a no-op or a lie).
pub fn core_const_ty(name: &str) -> Option<Ty> {
    Some(match name.trim_start_matches("::") {
        "Integer" => Ty::Int,
        "Float" => Ty::Float,
        "String" => Ty::Str,
        "Symbol" => Ty::Sym,
        "NilClass" => Ty::Nil,
        "TrueClass" | "FalseClass" => Ty::Bool,
        "Array" => Ty::Array(Box::new(Ty::Unknown)),
        "Hash" => Ty::Hash(Box::new(Ty::Unknown), Box::new(Ty::Unknown)),
        _ => return None,
    })
}

/// Whether `name` is a known core/stdlib constant, used to suppress
/// `E0104 unresolved-constant` on names the project index cannot resolve
/// (they are not user-defined, but they are real). Only matches simple
/// identifiers — qualified paths like `Float::INFINITY` are out of scope
/// here and are the constant resolver's job. ADDITIVE (bead ita-d2): a
/// hit on the mechanically generated `core_inventory_has_const` runs
/// first — it only ever ADDS true results (e.g. `SystemStackError`),
/// never removes one already covered by the hand-curated list below.
pub fn is_known_core_constant(name: &str) -> bool {
    core_inventory_has_const(name)
        || matches!(
        name,
        "Integer"
            | "SecureRandom"
            | "URI"
            | "OpenSSL"
            | "Base64"
            | "Digest"
            | "Logger"
            | "Tempfile"
            | "Pathname"
            | "StringIO"
            | "Timeout"
            | "Net"
            | "BigDecimal"
            | "Forwardable"
            | "Singleton"
            | "Observable"
            | "PP"
            | "PStore"
            | "Benchmark"
            | "Zlib"
            | "Socket"
            | "Resolv"
            | "String"
            | "Symbol"
            | "Float"
            | "Array"
            | "Hash"
            | "NilClass"
            | "TrueClass"
            | "FalseClass"
            | "Object"
            | "Kernel"
            | "Comparable"
            | "Enumerable"
            | "Struct"
            | "Math"
            | "ENV"
            | "ARGV"
            | "STDIN"
            | "STDOUT"
            | "STDERR"
            | "RUBY_VERSION"
            | "BasicObject"
            | "Module"
            | "Class"
            | "Proc"
            | "Range"
            | "Regexp"
            | "Exception"
            | "StandardError"
            | "ArgumentError"
            | "RuntimeError"
            | "TypeError"
            | "NameError"
            | "NoMethodError"
            | "KeyError"
            | "IndexError"
            | "StopIteration"
            | "NotImplementedError"
            | "IO"
            | "File"
            | "Dir"
            | "Time"
            | "Date"
            | "DateTime"
            | "Rational"
            | "Complex"
            | "Numeric"
            | "Set"
            | "OpenStruct"
            | "JSON"
            | "YAML"
            | "CSV"
            | "Marshal"
            | "ObjectSpace"
            | "GC"
            | "Signal"
            | "Process"
            | "Thread"
            | "Mutex"
            | "Queue"
            | "Random"
            | "Encoding"
            | "Data"
            | "Method"
            | "UnboundMethod"
            | "Binding"
            | "Fiber"
            | "Enumerator"
            | "Ractor"
            | "Warning"
            | "Errno"
            | "SystemExit"
            | "Interrupt"
            | "ScriptError"
            | "LoadError"
            | "SyntaxError"
            | "SecurityError"
            | "FrozenError"
            | "EncodingError"
            | "ZeroDivisionError"
            | "FloatDomainError"
            | "RangeError"
            | "LocalJumpError"
            | "EOFError"
            | "IOError"
            | "RegexpError"
            | "ThreadError"
            | "FiberError"
            | "ClosedQueueError"
            | "UncaughtThrowError"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_plus_returns_int() {
        let m = core_method(CoreClass::Integer, "+").unwrap();
        assert_eq!(
            m,
            CoreMethod {
                min_args: 1,
                max_args: Some(1),
                ret: CoreRet::Int
            }
        );
    }

    #[test]
    fn str_plus_returns_str() {
        let m = core_method(CoreClass::Str, "+").unwrap();
        assert_eq!(
            m,
            CoreMethod {
                min_args: 1,
                max_args: Some(1),
                ret: CoreRet::Str
            }
        );
    }

    #[test]
    fn array_push_is_varargs_self_same() {
        let m = core_method(CoreClass::Array, "push").unwrap();
        assert_eq!(m.min_args, 0);
        assert_eq!(m.max_args, None);
        assert_eq!(m.ret, CoreRet::SelfSame);
    }

    #[test]
    fn hash_keys_returns_key_array() {
        let m = core_method(CoreClass::Hash, "keys").unwrap();
        assert_eq!(m.ret, CoreRet::KeyArray);
    }

    #[test]
    fn array_first_returns_elem() {
        let m = core_method(CoreClass::Array, "first").unwrap();
        assert_eq!(m.min_args, 0);
        assert_eq!(m.max_args, Some(1));
        assert_eq!(m.ret, CoreRet::Elem);
    }

    #[test]
    fn object_respond_to_allows_optional_arg() {
        let m = core_method(CoreClass::Object, "respond_to?").unwrap();
        assert_eq!(m.min_args, 1);
        assert_eq!(m.max_args, Some(2));
    }

    #[test]
    fn unknown_method_is_none_not_a_panic() {
        assert_eq!(core_method(CoreClass::Integer, "nmae"), None);
        assert_eq!(core_method(CoreClass::Object, "totally_made_up"), None);
    }

    #[test]
    fn core_class_of_maps_concrete_types() {
        assert_eq!(core_class_of(&Ty::Int), Some(CoreClass::Integer));
        assert_eq!(
            core_class_of(&Ty::Array(Box::new(Ty::Int))),
            Some(CoreClass::Array)
        );
        assert_eq!(core_class_of(&Ty::Unknown), None);
        assert_eq!(core_class_of(&Ty::Union(vec![Ty::Int, Ty::Str])), None);
    }

    #[test]
    fn known_core_constants() {
        assert!(is_known_core_constant("Comparable"));
        assert!(is_known_core_constant("ENV"));
        assert!(!is_known_core_constant("TotallyMadeUp"));
    }
}
