//! Static extraction of a deliberately small Sorbet signature grammar.
//!
//! Prism supplies call-chain and argument boundaries; no Ruby is executed.
//! Type expressions are retained verbatim and resolved separately, with the
//! declaration's lexical nesting. Unsupported members remain `Unknown`, while
//! known Array/Hash categories are retained. This is not a full Sorbet checker:
//! overload selection, generics, proc types, bind and type parameters are not
//! modeled. Adjacency and rejection of multiple sigs belong to the index walker.

use ruby_prism::{CallNode, Node};

use crate::index::{ConstFallback, ProjectIndex};
use crate::types::{ClassId, SigOnly, Ty};

/// A single, unambiguous Sorbet contract. Parameter names are Ruby names, never
/// positional guesses. `void` discards the result; it does not promise `nil`.
/// `unchecked` records a written `.checked(:never)` / `.checked(:tests)`:
/// sorbet-runtime does not raise on that sig in the running program, so the
/// index never lets it accuse or type anything (`PendingSig::Unusable`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SorbetSig {
    pub params: Vec<(String, String)>,
    pub ret: Option<String>,
    pub void: bool,
    pub unchecked: bool,
}

/// Extract a bare `sig` / `sig(:final)` block with one builder call chain.
/// Foreign receivers, nested blocks, duplicate clauses, dynamic parameter keys
/// and unknown builder operations fail closed. Known modifiers are metadata,
/// not permission to infer additional type relationships.
pub fn extract_sig(node: &Node<'_>) -> Option<SorbetSig> {
    let call = node.as_call_node()?;
    if call.name().as_slice() != b"sig" || call.receiver().is_some() {
        return None;
    }
    if call.arguments().is_some() && !symbol_arg_is(&call, &[b"final"]) {
        return None;
    }
    let block = call.block()?.as_block_node()?;
    if block.parameters().is_some() {
        return None;
    }
    let body = block.body()?.as_statements_node()?;
    let mut statements = body.body().iter();
    let current = statements.next()?;
    if statements.next().is_some() {
        return None;
    }
    let mut sig = SorbetSig {
        params: Vec::new(),
        ret: None,
        void: false,
        unchecked: false,
    };
    walk_clause_chain(current, &mut sig)?;
    (sig.ret.is_some() || sig.void).then_some(sig)
}

/// Walk the builder chain from the OUTERMOST clause inward through each
/// receiver — `returns(X).params(...)` arrives as `returns` wrapping
/// `params`. A repeated clause, a block, or any call operator other than a
/// plain `.` rejects the whole signature.
fn walk_clause_chain(mut current: Node<'_>, sig: &mut SorbetSig) -> Option<()> {
    let mut seen = 0u8;
    loop {
        let builder = current.as_call_node()?;
        if builder.block().is_some()
            || builder.call_operator_loc().is_some_and(|loc| loc.as_slice() != b".")
        {
            return None;
        }
        let clause = extract_clause(&builder, sig)?;
        if seen & clause != 0 {
            return None;
        }
        seen |= clause;
        match builder.receiver() {
            Some(receiver) => current = receiver,
            None => return Some(()),
        }
    }
}

/// The return and void clauses share a bit, rejecting duplicates and mixed
/// returns/void contracts during the receiver walk.
fn extract_clause(call: &CallNode<'_>, sig: &mut SorbetSig) -> Option<u8> {
    match call.name().as_slice() {
        b"returns" => {
            let arg = single_arg(call)?;
            if arg.as_splat_node().is_some() || arg.as_keyword_hash_node().is_some() {
                return None;
            }
            sig.ret = Some(source_text(&arg)?);
            Some(1)
        }
        b"void" if no_args(call) => {
            sig.void = true;
            Some(1)
        }
        b"params" => {
            sig.params = extract_params(call)?;
            Some(2)
        }
        b"checked" if symbol_arg_is(call, &[b"always", b"tests", b"never"]) => {
            sig.unchecked = !symbol_arg_is(call, &[b"always"]);
            Some(64)
        }
        _ => modifier_clause(call),
    }
}

/// Argument-free modifiers: metadata, never a type.
fn modifier_clause(call: &CallNode<'_>) -> Option<u8> {
    match call.name().as_slice() {
        b"abstract" if no_args(call) => Some(4),
        b"override" if no_args(call) => Some(8),
        b"overridable" if no_args(call) => Some(16),
        b"final" if no_args(call) => Some(32),
        // `.on_failure(...)` (W7) only hands its arguments to
        // `T::Configuration.call_validation_error_handler`, whose default
        // raises; a project that installs another handler references
        // `T::Configuration` and is project-wide inert already.
        b"on_failure" => Some(128),
        _ => None,
    }
}

fn no_args(call: &CallNode<'_>) -> bool {
    call.arguments().is_none_or(|args| args.arguments().iter().next().is_none())
}

fn single_arg<'pr>(call: &CallNode<'pr>) -> Option<Node<'pr>> {
    let args = call.arguments()?;
    let mut iter = args.arguments().iter();
    let arg = iter.next()?;
    iter.next().is_none().then_some(arg)
}

/// The sole argument is one of the literal symbols `allowed` lists —
/// `sig(:final)`, `checked(:always)`. A symbol node borrows the argument it
/// came from, so the comparison happens here rather than handing a reference
/// back out of the temporary that owns it. Anything else is `false`, which
/// every caller reads as "reject", including a non-symbol argument.
fn symbol_arg_is(call: &CallNode<'_>, allowed: &[&[u8]]) -> bool {
    single_arg(call).is_some_and(|arg| {
        arg.as_symbol_node().is_some_and(|sym| allowed.contains(&sym.unescaped()))
    })
}

fn source_text(node: &Node<'_>) -> Option<String> {
    Some(std::str::from_utf8(node.location().as_slice()).ok()?.trim().to_owned())
}

fn extract_params(call: &CallNode<'_>) -> Option<Vec<(String, String)>> {
    if no_args(call) {
        return Some(Vec::new());
    }
    let hash = single_arg(call)?.as_keyword_hash_node()?;
    let mut params = Vec::new();
    for element in &hash.elements() {
        let assoc = element.as_assoc_node()?;
        let symbol = assoc.key().as_symbol_node()?;
        let name = std::str::from_utf8(symbol.unescaped()).ok()?;
        let mut chars = name.bytes();
        if !chars.next().is_some_and(|c| c.is_ascii_lowercase() || c == b'_')
            || !chars.all(|c| c.is_ascii_alphanumeric() || c == b'_')
            || params.iter().any(|(existing, _)| existing == name)
        {
            return None;
        }
        params.push((name.to_owned(), source_text(&assoc.value())?));
    }
    Some(params)
}

/// Index-blind mapping retained for consumers that only know core types.
/// Nominal project types and every unsupported shape remain `Unknown`.
pub fn sorbet_ret_ty(expr: &str) -> Ty {
    consumer_ty(parse_ty_leaf(expr, &|_| None))
}

/// Root-scoped return mapping for consumers without declaration nesting.
/// A consumer never sees a `Ty::Contract`: it types no receiver.
pub fn resolve_ret_ty(expr: Option<&str>, index: &ProjectIndex) -> Ty {
    expr.map_or(Ty::Unknown, |expr| consumer_ty(resolve_sig_ty(expr, index, &[])))
}

/// What a consumer may learn from a declared type: every `Ty::Contract`
/// (W6) is `Unknown`, and an `Unknown` union member absorbs the union.
pub fn consumer_ty(t: Ty) -> Ty {
    match t {
        Ty::Contract(_) => Ty::Unknown,
        Ty::Union(parts) => parts.into_iter().map(consumer_ty).reduce(Ty::union).unwrap_or(Ty::Unknown),
        Ty::Array(e) => Ty::Array(Box::new(consumer_ty(*e))),
        Ty::Hash(k, v) => Ty::Hash(Box::new(consumer_ty(*k)), Box::new(consumer_ty(*v))),
        other => other,
    }
}

/// `T.type_alias { X }` chains longer than this are `Unknown`.
const TYPE_ALIAS_DEPTH: usize = 8;

/// Resolve supported type expressions in the declaration's lexical scope.
/// Absolute `::` names bypass nesting, including inside compounds. Only literal
/// constant paths reach the index; calls, generics and Sorbet special types do
/// not become nominal classes just because a similarly named class exists.
/// A constant written as `T.type_alias { X }` (W6) resolves to `X`, read in
/// the alias's own lexical scope: sorbet-runtime checks the aliased type.
pub fn resolve_sig_ty(expr: &str, index: &ProjectIndex, nesting: &[String]) -> Ty {
    resolve_sig_ty_at(expr, index, nesting, 0)
}

fn resolve_sig_ty_at(expr: &str, index: &ProjectIndex, nesting: &[String], depth: usize) -> Ty {
    parse_ty_leaf(expr, &|name| {
        let bare = name.strip_prefix("::").unwrap_or(name);
        if !constant_path(bare) || bare.starts_with("T::") {
            return None;
        }
        if let Some((alias_nesting, body)) = index.find_type_alias(nesting, name) {
            return Some(if depth < TYPE_ALIAS_DEPTH {
                resolve_sig_ty_at(body, index, alias_nesting, depth + 1)
            } else {
                Ty::Unknown
            });
        }
        named_const_ty(name, bare, index, nesting)
    })
}

/// A literal constant path that is not a type alias.
fn named_const_ty(name: &str, bare: &str, index: &ProjectIndex, nesting: &[String]) -> Option<Ty> {
    // A qualified path still looks up its FIRST segment lexically.
    // resolve_const's dynamic-constant guard only covers simple names,
    // so reject a shadowed prefix before accepting its global fallback.
    if !name.starts_with("::") && shadowed_prefix(bare, index, nesting) {
        return Some(Ty::Unknown);
    }
    let ty = match index.resolve_const(nesting, name) {
        Some(id) => nominal_ty(id, index),
        None => scalar_ty(bare)?,
    };
    if ty == Ty::Unknown || name.starts_with("::") {
        return Some(ty);
    }
    Some(match inherited_fallback(bare, index, nesting) {
        ConstFallback::Shadowed => Ty::Unknown,
        ConstFallback::Opaque if matches!(ty, Ty::Instance(_)) => Ty::Unknown,
        _ => ty,
    })
}

/// Once no lexical scope carries the first segment, Ruby asks the crefs'
/// ancestors before the top level. A name a known ancestor namespace
/// carries is `Unknown`; a project class reached through the top-level
/// fallback also needs every cref's ancestry to be known. Core scalars
/// survive opaque (gem) ancestry: only a proven shadow retracts them.
fn inherited_fallback(name: &str, index: &ProjectIndex, nesting: &[String]) -> ConstFallback {
    let first = name.split("::").next().unwrap_or(name);
    if nesting.iter().any(|level| index.by_path.contains_key(&format!("{level}::{first}"))) {
        return ConstFallback::Proven;
    }
    index.const_fallback(nesting, first, index.by_path.get(first).copied())
}

/// A project class id in its sig meaning. A project reopen of a core
/// constant (`class Hash`, `module Comparable`, `class Object`) is still the
/// core constant: a scalar keeps its scalar type and the rest stay `Unknown`,
/// never an `Instance` no core value is compatible with. So does a project
/// module mixed into a core class, which core values satisfy invisibly.
fn nominal_ty(id: ClassId, index: &ProjectIndex) -> Ty {
    let path = &index.class(id).path;
    if let Some(ty) = scalar_ty(path) {
        return ty;
    }
    if crate::core::is_known_core_constant(path) {
        return Ty::Unknown;
    }
    if mixed_into_core(id, index) {
        return Ty::Unknown;
    }
    Ty::Instance(id)
}

fn mixed_into_core(id: ClassId, index: &ProjectIndex) -> bool {
    index.class(id).is_module && crate::core::core_namespace_names().iter().any(|core| {
        index.by_path.get(*core).is_some_and(|&reopen| reopen != id && index.ancestors(reopen).0.contains(&id))
    })
}

fn shadowed_prefix(name: &str, index: &ProjectIndex, nesting: &[String]) -> bool {
    let first = name.split("::").next().unwrap_or(name);
    for level in nesting.iter().rev() {
        if index.by_path.get(level).is_some_and(|&id| {
            index.class(id).consts.iter().any(|constant| constant == first)
        }) {
            return true;
        }
        // A nearer real namespace takes precedence over assignments farther
        // out. Do not mistake an unrelated outer binding for its shadow.
        if index.by_path.contains_key(&format!("{level}::{first}")) {
            return false;
        }
    }
    false
}

fn constant_path(s: &str) -> bool {
    s.split("::").all(|part| {
        let mut bytes = part.bytes();
        bytes.next().is_some_and(|c| c.is_ascii_uppercase())
            && bytes.all(|c| c.is_ascii_alphanumeric() || c == b'_')
    })
}

fn parse_ty_leaf(expr: &str, resolve: &dyn Fn(&str) -> Option<Ty>) -> Ty {
    let s = expr.trim();
    let bare = s.strip_prefix("::").unwrap_or(s);
    compound_ty_leaf(bare, resolve)
        .or_else(|| resolve(s))
        .or_else(|| scalar_ty(bare))
        .unwrap_or(Ty::Unknown)
}

/// The leaf names: a sorbet type that is just a class name we model
/// directly. `None` means "not a leaf we know", never "unknown type" —
/// only `parse_ty_leaf` decides that, after `compound_ty_leaf` and the
/// resolver have also missed.
fn scalar_ty(s: &str) -> Option<Ty> {
    Some(match s {
        "Integer" => Ty::Int,
        "Float" => Ty::Float,
        "String" => Ty::Str,
        "Symbol" => Ty::Sym,
        "T::Boolean" | "TrueClass" | "FalseClass" => Ty::Bool,
        "NilClass" => Ty::Nil,
        _ => return None,
    })
}

/// Unknown union members absorb the union; unknown collection members stay
/// unknown without erasing the independently known Array/Hash category.
fn compound_ty_leaf(s: &str, resolve: &dyn Fn(&str) -> Option<Ty>) -> Option<Ty> {
    if let Some(inner) = strip_call(s, "T.nilable") {
        let parts = split_top_level(inner, ',')?;
        return Some(match parts.as_slice() {
            [element] => Ty::union(parse_ty_leaf(element, resolve), Ty::Nil),
            _ => Ty::Unknown,
        });
    }
    container_ty_leaf(s, resolve)
        .or_else(|| union_ty_leaf(s, resolve))
        .or_else(|| intersection_ty_leaf(s, resolve))
        .or_else(|| sig_only_leaf(s))
        .or_else(|| literal_shape_leaf(s))
}

/// `T.all(A, B, ...)` (W6): sorbet-runtime requires EVERY member, so any
/// one readable member is a sound stand-in — a value failing it raises, and
/// a value that passed is one. The first member that is not `Unknown` is
/// taken; a single member is not an intersection and stays `Unknown`.
fn intersection_ty_leaf(s: &str, resolve: &dyn Fn(&str) -> Option<Ty>) -> Option<Ty> {
    let parts = split_top_level(strip_call(s, "T.all")?, ',')?;
    if parts.len() < 2 {
        return Some(Ty::Unknown);
    }
    Some(parts.into_iter().map(|part| parse_ty_leaf(part, resolve)).find(|t| *t != Ty::Unknown).unwrap_or(Ty::Unknown))
}

/// The Sorbet types sorbet-runtime enforces that no value type models
/// (W6): `T.noreturn`, the class-object types, and the core categories
/// whose type arguments the runtime erases (`T::Set[X]` checks
/// `is_a?(Set)` and nothing about `X`). A wrong member count is `Unknown`.
fn sig_only_leaf(s: &str) -> Option<Ty> {
    if s == "T.noreturn" {
        return Some(Ty::Contract(SigOnly::NoReturn));
    }
    let one = |inner: &str| split_top_level(inner, ',').is_some_and(|parts| parts.len() == 1);
    if let Some(inner) = strip_call(s, "T.class_of")
        .or_else(|| strip_index(s, "T::Class"))
        .or_else(|| strip_index(s, "T::Module"))
    {
        return Some(if one(inner) { Ty::Contract(SigOnly::ClassObject) } else { Ty::Unknown });
    }
    const CORE: [(&str, &str); 5] = [
        ("T::Set", "Set"),
        ("T::Range", "Range"),
        ("T::Enumerable", "Enumerable"),
        ("T::Enumerator::Lazy", "Enumerator::Lazy"),
        ("T::Enumerator", "Enumerator"),
    ];
    CORE.iter().find_map(|(spelling, class)| {
        let inner = strip_index(s, spelling)?;
        Some(if one(inner) { Ty::Contract(SigOnly::Core(class)) } else { Ty::Unknown })
    })
}

/// A tuple `[A, B]` or a shape `{a: A}` (W6). sorbet-runtime checks the
/// members too, but only the category is kept here: an `Array` / a `Hash`
/// with unknown contents, exactly what `T::Array[X]` / `T::Hash[K, V]`
/// already give.
fn literal_shape_leaf(s: &str) -> Option<Ty> {
    if let Some(inner) = s.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
        return Some(if inner.trim().is_empty() || split_top_level(inner, ',').is_some() {
            Ty::Array(Box::new(Ty::Unknown))
        } else {
            Ty::Unknown
        });
    }
    let inner = s.strip_prefix('{')?.strip_suffix('}')?;
    Some(if inner.trim().is_empty() || split_top_level(inner, ',').is_some() {
        Ty::Hash(Box::new(Ty::Unknown), Box::new(Ty::Unknown))
    } else {
        Ty::Unknown
    })
}

/// The two bracketed containers. A member count the spelling does not
/// support is `Unknown`, exactly as an unreadable inner expression is.
fn container_ty_leaf(s: &str, resolve: &dyn Fn(&str) -> Option<Ty>) -> Option<Ty> {
    if let Some(inner) = strip_index(s, "T::Array") {
        let parts = split_top_level(inner, ',')?;
        return Some(match parts.as_slice() {
            [element] => Ty::Array(Box::new(parse_ty_leaf(element, resolve))),
            _ => Ty::Unknown,
        });
    }
    let parts = split_top_level(strip_index(s, "T::Hash")?, ',')?;
    Some(match parts.as_slice() {
        [k, v] => Ty::Hash(
            Box::new(parse_ty_leaf(k, resolve)),
            Box::new(parse_ty_leaf(v, resolve)),
        ),
        _ => Ty::Unknown,
    })
}

/// `T.any(...)` — a single member is not a union and stays `Unknown`.
fn union_ty_leaf(s: &str, resolve: &dyn Fn(&str) -> Option<Ty>) -> Option<Ty> {
    let parts = split_top_level(strip_call(s, "T.any")?, ',')?;
    if parts.len() < 2 {
        return Some(Ty::Unknown);
    }
    let mut tys = parts.into_iter().map(|part| parse_ty_leaf(part, resolve));
    let first = tys.next()?;
    Some(tys.fold(first, Ty::union))
}

/// Strip the outer call spelling. The recursive leaf parser and compound
/// splitter reject invalid inner syntax, including unmatched delimiters.
fn strip_call<'a>(s: &'a str, name: &str) -> Option<&'a str> {
    let rest = s.strip_prefix(name)?.trim_start();
    rest.strip_prefix('(')?.strip_suffix(')')
}

/// `name[...]` — the `T::Array[X]` / `T::Hash[K, V]` bracket form.
fn strip_index<'a>(s: &'a str, name: &str) -> Option<&'a str> {
    let rest = s.strip_prefix(name)?.trim_start();
    rest.strip_prefix('[')?.strip_suffix(']')
}

/// Preserve empty parts as a rejection, and check matching delimiter kinds.
/// Unsupported strings/blocks never need a textual comma heuristic: their
/// nodes were already captured intact by Prism and resolve to `Unknown`.
fn split_top_level(s: &str, sep: char) -> Option<Vec<&str>> {
    let mut parts = Vec::new();
    let mut stack = Vec::new();
    let mut start = 0usize;
    for (i, c) in s.char_indices() {
        if track_delimiter(c, &mut stack)? {
            continue;
        }
        if c == sep && stack.is_empty() {
            let part = s[start..i].trim();
            if part.is_empty() {
                return None;
            }
            parts.push(part);
            start = i + c.len_utf8();
        } else if !(c.is_ascii_alphanumeric() || c.is_ascii_whitespace() || "_:.,".contains(c)) {
            return None;
        }
    }
    let last = s[start..].trim();
    if !stack.is_empty() || last.is_empty() {
        return None;
    }
    parts.push(last);
    Some(parts)
}

/// `Some(true)` — `c` opened or correctly closed a nesting delimiter and
/// the caller has nothing left to decide about it. `Some(false)` — an
/// ordinary character. `None` — a closer that does not match the opener it
/// meets, which rejects the whole expression.
fn track_delimiter(c: char, stack: &mut Vec<char>) -> Option<bool> {
    match c {
        '(' => stack.push(')'),
        '[' => stack.push(']'),
        '{' => stack.push('}'),
        ')' | ']' | '}' => return (stack.pop() == Some(c)).then_some(true),
        _ => return Some(false),
    }
    Some(true)
}
