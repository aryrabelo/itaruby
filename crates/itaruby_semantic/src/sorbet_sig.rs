//! Sorbet `sig { ... }` return-type mapping (bead ita-uh1). Maps the raw
//! source text of a `.returns(...)` argument — captured verbatim by
//! `index.rs`'s `DefWalker` (see `MethodDef::sorbet_ret` and
//! `extract_sig_return`), never reparsed as Ruby — to this crate's `Ty`.
//!
//! Hand-written recursive-descent scanner over the type-expression text,
//! same spirit as `structure_sql.rs`'s SQL scanner: stdlib only, no crate
//! addition, deliberately narrow. **Anything this parser doesn't
//! recognize maps to `Ty::Unknown`, never a guess** — that is what
//! preserves invariant #1 here: this function is purely name-based — it
//! never sees the `ProjectIndex`, so it has no way to resolve a class
//! name (`::Quantity`, an app model reopened by Tapioca's `dsl/` RBIs) to
//! a `ClassId`, and guessing one would be a false positive. Read that as
//! "this function is not the place to resolve a project class name", not
//! as "project types are permanently out of scope": an index-aware layer
//! over `dsl/`-generated sigs (Tapioca's per-app-model RBIs, a separate
//! bead) can sit on top of this one and resolve `Instance`/`Class` where
//! it actually has the index to do so safely. This module's own contract
//! stays fixed regardless: it must NEVER produce `Ty::Instance`/
//! `Ty::Class` itself, only a core scalar, a collection of core types, or
//! `Unknown`.
//!
//! Arity is deliberately out of scope (bead ita-uh1's contract, not this
//! module's call): only the `.returns(...)` argument is ever captured by
//! `index.rs` in the first place — `params(...)`/`void`/`override`/
//! `abstract` never reach this function.

use crate::types::Ty;

/// Map a sorbet return-type expression's source text to `Ty`.
/// `Ty::Unknown` for every unrecognized shape — `T.untyped`,
/// `T.self_type`, `T.proc`, `T::Set[...]`, a class name (project or gem —
/// this function has no index to resolve one to a `ClassId`; see the
/// module doc comment), or any other construct this scanner doesn't
/// model. Covers exactly the sig shapes measured in the reference
/// corpus's `gems/` RBIs (bead ita-uh1); deliberately does not chase rare
/// gem-only forms (`T.class_of`, `T.proc`, generics) that never showed up
/// there.
pub fn sorbet_ret_ty(expr: &str) -> Ty {
    parse_ty(expr)
}

/// Bead ita-tjr: index-aware layer over `sorbet_ret_ty` (see this
/// module's doc comment for why that function itself never resolves a
/// name). `None`/absent sig text -> `Ty::Unknown`. Otherwise the same
/// scalar/compound grammar as `sorbet_ret_ty` runs first; only where a
/// LEAF of that grammar would fall through to `Unknown` does this layer
/// try the leaf text against `index.resolve_const(&[], name)` (empty
/// nesting — sig text carries no lexical scope of its own) and, on a hit,
/// produce `Ty::Instance(ClassId)`. Resolution happens at the leaves,
/// not on the whole input, so nesting (`T.nilable(::Quantity)`,
/// `T::Array[Quantity]`) still reaches the inner name. A name that does
/// not resolve in the index stays `Ty::Unknown` — never a guess, same
/// invariant as `sorbet_ret_ty`; this is also the only way this bead can
/// produce a diagnostic downstream (a wrong resolution -> a wrong
/// `ClassId` -> a possible false E0101), so a miss here MUST stay a miss.
pub fn resolve_ret_ty(expr: Option<&str>, index: &crate::index::ProjectIndex) -> Ty {
    let Some(expr) = expr else {
        return Ty::Unknown;
    };
    parse_ty_leaf(expr, &|name| index.resolve_const(&[], name).map(Ty::Instance))
}

/// `parse_ty`, parameterized by a leaf resolver tried only after
/// `scalar_ty`/`compound_ty` both miss. `sorbet_ret_ty` itself is
/// `parse_ty_leaf` with a resolver that always misses (`|_| None`),
/// unchanged in observable behavior — this function does not duplicate
/// the grammar, it threads one extra fallback through the existing
/// recursion (`compound_ty_leaf` below).
fn parse_ty_leaf(expr: &str, resolve: &dyn Fn(&str) -> Option<Ty>) -> Ty {
    let s = expr.trim();
    let s = s.strip_prefix("::").unwrap_or(s).trim();
    scalar_ty(s)
        .or_else(|| compound_ty_leaf(s, resolve))
        .or_else(|| resolve(s))
        .unwrap_or(Ty::Unknown)
}

fn parse_ty(expr: &str) -> Ty {
    parse_ty_leaf(expr, &|_| None)
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

/// The four compound forms, each recursing through `parse_ty_leaf` so
/// nesting (`T.nilable(T::Array[String])`) works at any depth AND the
/// leaf resolver reaches every inner name, not just a top-level one. A
/// malformed inner shape (`T::Hash` without exactly two parts) is
/// `Ty::Unknown`, not a guess at the missing half. `compound_ty` (used by
/// the index-blind `parse_ty`) is this with a resolver that always
/// misses — see `parse_ty`.
fn compound_ty_leaf(s: &str, resolve: &dyn Fn(&str) -> Option<Ty>) -> Option<Ty> {
    if let Some(inner) = strip_call(s, "T.nilable") {
        return Some(Ty::union(parse_ty_leaf(inner, resolve), Ty::Nil));
    }
    if let Some(inner) = strip_index(s, "T::Array") {
        return Some(Ty::Array(Box::new(parse_ty_leaf(inner, resolve))));
    }
    if let Some(inner) = strip_index(s, "T::Hash") {
        return Some(match split_top_level(inner, ',').as_slice() {
            [k, v] => Ty::Hash(
                Box::new(parse_ty_leaf(k, resolve)),
                Box::new(parse_ty_leaf(v, resolve)),
            ),
            _ => Ty::Unknown,
        });
    }
    let inner = strip_call(s, "T.any")?;
    let mut tys = split_top_level(inner, ',').into_iter().map(|p| parse_ty_leaf(p, resolve));
    let first = tys.next()?;
    Some(tys.fold(first, Ty::union))
}

/// `name(...)` — the text between the outermost parens, only when `s` is
/// exactly that call: nothing before `name`, nothing after the matching
/// close paren. `None` for a mere prefix collision (`"Integerish(...)"`
/// does not strip as `"Integer"`) or any other shape.
fn strip_call<'a>(s: &'a str, name: &str) -> Option<&'a str> {
    let rest = s.strip_prefix(name)?.trim_start();
    rest.strip_prefix('(')?.strip_suffix(')')
}

/// `name[...]` — the `T::Array[X]` / `T::Hash[K, V]` bracket form.
fn strip_index<'a>(s: &'a str, name: &str) -> Option<&'a str> {
    let rest = s.strip_prefix(name)?.trim_start();
    rest.strip_prefix('[')?.strip_suffix(']')
}

/// Splits on `sep` at bracket/paren depth 0 only, so
/// `T::Hash[Symbol, T.nilable(Integer)]`'s inner text splits into exactly
/// two parts instead of three. Each part is trimmed; empty parts (a
/// trailing separator) are dropped.
fn split_top_level(s: &str, sep: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    for (i, c) in s.char_indices() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            c if c == sep && depth == 0 => {
                parts.push(s[start..i].trim());
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(s[start..].trim());
    parts.into_iter().filter(|p| !p.is_empty()).collect()
}
