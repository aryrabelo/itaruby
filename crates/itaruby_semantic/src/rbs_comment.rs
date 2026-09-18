//! Parser for inline RBS signature comments: `#: (Integer, ?String, foo: Symbol) -> Array[String]`
//! and every other documented form from sorbet.org/docs/rbs-support — bare
//! arrow (no params: `#: -> void`), positional parameter names (`Integer x`),
//! rest positional/keyword (`*Integer`, `**untyped`), postfix-nilable types
//! (`String?`, the real RBS form — prefix `?Type` is ALSO accepted, kept for
//! backward compatibility with earlier callers of this parser), block/proc
//! types (`{ (String) -> void }`, `?{ ... }`, `^(Integer) -> Integer`), and a
//! leading generic-method type-parameter list (`[T]`).
//!
//! A malformed sig is reported (E0105, Warning) and IGNORED — the method
//! falls back to inference. Never "give up on the whole method".
//!
//! Forms whose semantics `RbsParam`/`RbsTy` cannot represent without adding a
//! variant (which would break the exhaustive matches in `check.rs` — out of
//! this file's ownership) are parsed fully, so genuinely malformed input in
//! them still errors, then deliberately degraded rather than invented:
//! - `**Type [name]` (rest keyword): parsed and dropped — a rest keyword has
//!   no fixed name to key a `RbsParam::Keyword` lookup on, and inventing one
//!   risks colliding with a real keyword param of the same spelling.
//! - `*Type [name]` (rest positional): kept as `RbsParam::Optional` — the
//!   closest existing shape to "zero or more of this type", and harmless for
//!   arity (real arity comes from the Ruby `def`'s own params, never from
//!   this list — see `check.rs::check_arity`).
//! - Intersection types (`A & B`), proc types (`^(...) -> T`), tuple types
//!   (`[Foo, Bar]`), and class singleton types (`singleton(Foo)`) all degrade
//!   to `RbsTy::Generic` with an untagged name (`"__tuple"`/`"singleton"`) or
//!   `RbsTy::Simple("untyped")` — `check.rs::rbs_to_ty`'s existing
//!   `RbsTy::Generic` match already falls through any name besides
//!   `"Array"`/`"Hash"` to `Ty::Unknown`, the same fallback `schema.rs`
//!   already uses for a column type it doesn't model.
//! - Block/proc clauses on a method sig (`{ ... }` / `?{ ... }`) are
//!   parsed for validation, then discarded entirely: they never
//!   contribute to `RbsSig::params`. A leading `[T, ...]` type-parameter
//!   list IS retained (bead ita-s12, `RbsSig::type_params`) — see that
//!   field's doc comment for why.
//! - `(?)` as a block's own parameter list (`{ (?) -> T }` / `?{ (?) -> T }`)
//!   — the RBS spec's dedicated "untyped function" placeholder (see
//!   github.com/ruby/rbs `docs/syntax.md`'s `_block?_` production: `?` in
//!   place of the whole parameter list means "params are untyped", never a
//!   parse error). Measured FP: tapioca's
//!   `lib/tapioca/helpers/test/isolation.rb:30,75` —
//!   `?{ (?) -> untyped } -> String`, accepted by `srb tc` (bead ita-hfn,
//!   the last residual of ita-p24's block-type coverage). Parsed and
//!   discarded exactly like a block's real param list already was — the
//!   whole clause never contributes to `RbsSig::params` regardless, so no
//!   new `RbsParam`/`RbsTy` shape is needed. The identical placeholder is
//!   ALSO valid, per spec, as a method's own top-level param list
//!   (`(?) -> T`) and inside a proc type (`^(?) -> T`) — both are
//!   DELIBERATELY left unimplemented here: no measured corpus occurrence of
//!   either shape has turned up (ita-hfn's only cited site is the block
//!   form), so widening now would be speculative rather than FP-driven.
//!   `(?)` written at either of those two positions still raises E0105
//!   today; expand when a real occurrence justifies it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RbsSig {
    pub params: Vec<RbsParam>,
    pub ret: RbsTy,
    /// Names declared by a leading `[T, U, ...]` generic-method
    /// type-parameter list (bead ita-s12). `check.rs::rbs_to_ty` binds
    /// every one of these to `Ty::Unknown` for the scope of THIS sig,
    /// rather than resolving `T` as if it named a real project/core
    /// class — sound by construction (invariant #1: `Unknown` never
    /// diagnoses). Measured false E0103: ruby-lsp's
    /// `#: [T] (String, T) -> T` (`Document#cache_set`), rejected a
    /// `Array[Integer]` argument because the project vendors a real
    /// `T` module (sorbet-runtime's RBI stub) that `resolve_const`
    /// happily found. A sig with NO `[...]` list leaves this empty, so
    /// a nominal type genuinely named `T` outside a generic sig still
    /// resolves exactly as before — precedence is: inside a sig that
    /// declares `[T]`, `T` means the type variable, never the class.
    pub type_params: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum RbsParam {
    Required(RbsTy),
    Optional(RbsTy),
    Keyword { name: String, ty: RbsTy, required: bool },
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum RbsTy {
    /// `Integer`, `bool`, `void`, `untyped`, `nil`, `self`, or a project
    /// constant path like `Foo::Bar`.
    Simple(String),
    /// `Array[T]`, `Hash[K, V]` — 1 or 2 args.
    Generic(String, Vec<RbsTy>),
    Nilable(Box<RbsTy>),
    Union(Vec<RbsTy>),
}

/// Parse the text after `#:`, e.g. `(Integer) -> String`.
///
/// # Errors
///
/// Returns `Err` with a human-readable message when `input` does not match
/// the RBS method-type grammar this parser accepts.
pub fn parse_rbs_comment(input: &str) -> Result<RbsSig, String> {
    let mut p = Parser { s: input.as_bytes(), pos: 0 };
    p.skip_ws();
    let type_params = p.parse_type_params()?;
    p.skip_ws();
    let params = if p.eat(b'(') { p.param_list()? } else { Vec::new() };
    p.skip_ws();
    p.skip_block_type()?;
    p.skip_ws();
    if !(p.eat(b'-') && p.eat(b'>')) {
        return Err(format!("expected `->` at offset {}", p.pos));
    }
    let ret = p.ty()?;
    p.skip_ws();
    if p.pos != p.s.len() {
        return Err(format!("trailing input at offset {}", p.pos));
    }
    Ok(RbsSig { params, ret, type_params })
}

struct Parser<'a> {
    s: &'a [u8],
    pos: usize,
}

impl Parser<'_> {
    fn skip_ws(&mut self) {
        while self.pos < self.s.len() && (self.s[self.pos] == b' ' || self.s[self.pos] == b'\t') {
            self.pos += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.s.get(self.pos).copied()
    }

    fn eat(&mut self, b: u8) -> bool {
        if self.peek() == Some(b) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn ident(&mut self) -> Result<String, String> {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() || c == b'_' {
                self.pos += 1;
            } else if c == b':' && self.s.get(self.pos + 1) == Some(&b':') {
                self.pos += 2; // constant path separator
            } else {
                break;
            }
        }
        if self.pos == start {
            return Err(format!("expected identifier at offset {start}"));
        }
        Ok(String::from_utf8_lossy(&self.s[start..self.pos]).into_owned())
    }

    /// Optional `Ident name` / `?Type name` / `*Type name` trailing parameter
    /// name (sorbet.org/docs/rbs-support: "positional and rest parameter
    /// names are optional"). Consumed and discarded — this parser only
    /// tracks types, not names, for positional/rest params.
    fn skip_param_name(&mut self) {
        self.skip_ws();
        let save = self.pos;
        if self.ident().is_err() {
            self.pos = save;
        }
    }

    /// Leading generic-method type-parameter list, e.g. `[T]`, `[T, U]`,
    /// `[T < Bound]`, `[T = Default]`. Depth-counted so a bound/default
    /// containing its own `[...]` (e.g. `[T = Array[String]]`) still closes
    /// correctly. Bead ita-s12: returns the declared names themselves
    /// (`T`, `U`, ...) rather than discarding them — an identifier read
    /// right after `[` or a top-level `,` is a name; anything else in
    /// the list (a variance keyword, a bound, a default) is skipped like
    /// before, purely for validation.
    fn parse_type_params(&mut self) -> Result<Vec<String>, String> {
        self.skip_ws();
        if !self.eat(b'[') {
            return Ok(Vec::new());
        }
        let mut names = Vec::new();
        let mut depth = 1i32;
        let mut at_name_position = true;
        while depth > 0 {
            if depth == 1 && at_name_position {
                self.skip_ws();
                if let Ok(name) = self.ident() {
                    names.push(name);
                    at_name_position = false;
                    continue;
                }
            }
            match self.peek() {
                Some(b'[') => {
                    depth += 1;
                    self.pos += 1;
                    at_name_position = false;
                }
                Some(b']') => {
                    depth -= 1;
                    self.pos += 1;
                    at_name_position = false;
                }
                Some(b',') => {
                    self.pos += 1;
                    at_name_position = depth == 1;
                }
                Some(_) => {
                    self.pos += 1;
                    at_name_position = false;
                }
                None => return Err("unterminated type parameter list".to_string()),
            }
        }
        Ok(names)
    }

    /// `(param, param, ...)` after the opening `(` has already been eaten;
    /// consumes through the closing `)`.
    fn param_list(&mut self) -> Result<Vec<RbsParam>, String> {
        let mut params = Vec::new();
        self.skip_ws();
        if self.eat(b')') {
            return Ok(params);
        }
        loop {
            if let Some(p) = self.param()? {
                params.push(p);
            }
            self.skip_ws();
            if self.eat(b',') {
                self.skip_ws();
                continue;
            }
            if self.eat(b')') {
                break;
            }
            return Err(format!("expected `,` or `)` at offset {}", self.pos));
        }
        Ok(params)
    }

    /// One parameter. `Ok(None)` means "parsed successfully but nothing to
    /// record" — only rest keyword (`**Type`) takes that path.
    fn param(&mut self) -> Result<Option<RbsParam>, String> {
        self.skip_ws();
        if self.peek() == Some(b'*') {
            return self.rest_param();
        }
        if self.eat(b'?') {
            return self.optional_param();
        }
        self.required_or_keyword_param()
    }

    /// Rest positional (`*Type [name]`, kept as `RbsParam::Optional`) or
    /// rest keyword (`**Type [name]`, dropped — see module doc).
    fn rest_param(&mut self) -> Result<Option<RbsParam>, String> {
        if self.s.get(self.pos + 1) == Some(&b'*') {
            self.pos += 2;
            self.ty()?;
            self.skip_param_name();
            return Ok(None);
        }
        self.pos += 1;
        let ty = self.ty()?;
        self.skip_param_name();
        Ok(Some(RbsParam::Optional(ty)))
    }

    /// A leading `?` was already eaten: `?foo: T` optional keyword, or
    /// `?T [name]` optional positional.
    fn optional_param(&mut self) -> Result<Option<RbsParam>, String> {
        self.skip_ws();
        let save = self.pos;
        if let Ok(name) = self.ident() {
            if self.peek() == Some(b':') && self.s.get(self.pos + 1) != Some(&b':') {
                self.pos += 1;
                let ty = self.ty()?;
                return Ok(Some(RbsParam::Keyword { name, ty, required: false }));
            }
        }
        self.pos = save;
        let ty = self.ty()?;
        self.skip_param_name();
        Ok(Some(RbsParam::Optional(ty)))
    }

    /// No leading `*`/`?`: a required keyword (`name: T`, lowercase name) or
    /// a required positional type, optionally named.
    fn required_or_keyword_param(&mut self) -> Result<Option<RbsParam>, String> {
        let save = self.pos;
        if let Ok(name) = self.ident() {
            if self.peek() == Some(b':') && self.s.get(self.pos + 1) != Some(&b':') {
                // lowercase name followed by single `:` = keyword param
                if name.as_bytes()[0].is_ascii_lowercase() || name.as_bytes()[0] == b'_' {
                    self.pos += 1;
                    let ty = self.ty()?;
                    return Ok(Some(RbsParam::Keyword { name, ty, required: true }));
                }
            }
        }
        self.pos = save;
        let ty = self.ty()?;
        self.skip_param_name();
        Ok(Some(RbsParam::Required(ty)))
    }

    /// Optional block/proc clause between the param list and `->`:
    /// `{ (Args) -> Ret }`, `?{ (Args) -> Ret }`, `{ -> Ret }`, with an
    /// optional `[self: Type]` before the arrow. Parsed for validation, then
    /// discarded — `RbsSig` has no field for it.
    fn skip_block_type(&mut self) -> Result<(), String> {
        self.skip_ws();
        let optional_marker = self.peek() == Some(b'?') && self.s.get(self.pos + 1) == Some(&b'{');
        if optional_marker {
            self.pos += 1;
        }
        if self.peek() != Some(b'{') {
            if optional_marker {
                return Err(format!("expected `{{` after `?` at offset {}", self.pos));
            }
            return Ok(());
        }
        self.pos += 1;
        self.skip_ws();
        if self.eat(b'(') {
            self.block_param_list()?;
            self.skip_ws();
        }
        self.skip_block_self_type()?;
        if !(self.eat(b'-') && self.eat(b'>')) {
            return Err(format!("expected `->` in block type at offset {}", self.pos));
        }
        self.ty()?;
        self.skip_ws();
        if !self.eat(b'}') {
            return Err(format!("expected `}}` at offset {}", self.pos));
        }
        Ok(())
    }

    /// Block-type parameter list: same as `param_list`, but additionally
    /// accepts the RBS "untyped function" placeholder `(?)` — see module
    /// doc's `(?)` entry (bead ita-hfn). The placeholder is only ever the
    /// WHOLE list, per spec, so a lone `?` immediately followed by `)` is
    /// the sole trigger; anything else (a real param, `?` mixed with a
    /// real param, a bare `?` followed by more input) falls through to
    /// ordinary `param_list`, which parses/rejects it exactly as before —
    /// this never widens what a real (non-placeholder) param list accepts.
    fn block_param_list(&mut self) -> Result<(), String> {
        self.skip_ws();
        let save = self.pos;
        if self.eat(b'?') {
            self.skip_ws();
            if self.eat(b')') {
                return Ok(());
            }
            self.pos = save;
        }
        self.param_list()?;
        Ok(())
    }

    /// Optional `[self: Type]` clause inside a block type.
    fn skip_block_self_type(&mut self) -> Result<(), String> {
        if !self.eat(b'[') {
            return Ok(());
        }
        self.skip_ws();
        let name = self.ident()?;
        if name != "self" {
            return Err(format!("expected `self:` in block self-type at offset {}", self.pos));
        }
        self.skip_ws();
        if !self.eat(b':') {
            return Err(format!("expected `:` at offset {}", self.pos));
        }
        self.ty()?;
        self.skip_ws();
        if !self.eat(b']') {
            return Err(format!("expected `]` at offset {}", self.pos));
        }
        self.skip_ws();
        Ok(())
    }

    fn ty(&mut self) -> Result<RbsTy, String> {
        let first = self.intersection_ty()?;
        self.skip_ws();
        if self.peek() != Some(b'|') {
            return Ok(first);
        }
        let mut parts = vec![first];
        while self.eat(b'|') {
            self.skip_ws();
            parts.push(self.intersection_ty()?);
            self.skip_ws();
        }
        Ok(RbsTy::Union(parts))
    }

    /// `&`-separated intersection. `RbsTy` has no variant for it (adding one
    /// would break `check.rs`'s exhaustive `rbs_to_ty` match, out of this
    /// file's ownership), so 2+ terms degrade honestly to `untyped`
    /// (`Ty::Unknown` downstream — never a false positive). Malformed input
    /// on either side of `&` still errors: every term is fully parsed.
    fn intersection_ty(&mut self) -> Result<RbsTy, String> {
        let first = self.primary()?;
        self.skip_ws();
        if self.peek() != Some(b'&') {
            return Ok(first);
        }
        while self.eat(b'&') {
            self.skip_ws();
            self.primary()?;
            self.skip_ws();
        }
        Ok(RbsTy::Simple("untyped".to_string()))
    }

    fn primary(&mut self) -> Result<RbsTy, String> {
        self.skip_ws();
        if self.eat(b'?') {
            return Ok(RbsTy::Nilable(Box::new(self.primary()?)));
        }
        if self.eat(b'^') {
            return self.proc_type();
        }
        if self.eat(b'(') {
            // Parenthesized grouping, e.g. `(Foo | Bar)`.
            let inner = self.ty()?;
            self.skip_ws();
            if !self.eat(b')') {
                return Err(format!("expected `)` at offset {}", self.pos));
            }
            return Ok(self.postfix_nilable(inner));
        }
        if self.eat(b'{') {
            return self.shape_type();
        }
        if self.eat(b'[') {
            // Tuple type `[Foo, Bar]`. Not representable in `RbsTy` (see
            // module doc) — the untagged name falls through `check.rs`'s
            // existing `RbsTy::Generic` catch-all to `untyped`.
            let args = self.bracketed_types()?;
            return Ok(self.postfix_nilable(RbsTy::Generic("__tuple".to_string(), args)));
        }
        let name = self.ident()?;
        if name == "singleton" && self.eat(b'(') {
            return self.singleton_type();
        }
        if self.eat(b'[') {
            let args = self.bracketed_types()?;
            if args.len() > 2 {
                return Err("generics support at most 2 type arguments".to_string());
            }
            return Ok(self.postfix_nilable(RbsTy::Generic(name, args)));
        }
        Ok(self.postfix_nilable(RbsTy::Simple(name)))
    }

    /// A leading `^` was already eaten: proc type `^(Args) -> Ret`, or a
    /// zero-arg `^-> Ret` (params optional, exactly like a bare-arrow method
    /// sig), e.g. as a param/return type. Not representable in `RbsTy` (see
    /// module doc) — parsed fully so a malformed proc type still errors,
    /// then degraded to `untyped`.
    fn proc_type(&mut self) -> Result<RbsTy, String> {
        self.skip_ws();
        if self.eat(b'(') {
            self.param_list()?;
            self.skip_ws();
        }
        if !(self.eat(b'-') && self.eat(b'>')) {
            return Err(format!("expected `->` in proc type at offset {}", self.pos));
        }
        self.ty()?;
        Ok(self.postfix_nilable(RbsTy::Simple("untyped".to_string())))
    }

    /// `singleton(` was already matched (the ident "singleton" plus its
    /// opening paren): class singleton type `singleton(Foo)`. Same
    /// catch-all fate as `proc_type`.
    fn singleton_type(&mut self) -> Result<RbsTy, String> {
        let inner = self.ty()?;
        self.skip_ws();
        if !self.eat(b')') {
            return Err(format!("expected `)` at offset {}", self.pos));
        }
        Ok(self.postfix_nilable(RbsTy::Generic("singleton".to_string(), vec![inner])))
    }

    /// A leading `{` was already eaten: shape/record type
    /// `{ key: Type, ... }`. Not representable in `RbsTy` (see module doc)
    /// — parsed fully so a malformed shape still errors, then degraded to
    /// `untyped`.
    fn shape_type(&mut self) -> Result<RbsTy, String> {
        self.skip_ws();
        if self.eat(b'}') {
            return Ok(self.postfix_nilable(RbsTy::Simple("untyped".to_string())));
        }
        loop {
            self.skip_ws();
            self.ident()?;
            self.skip_ws();
            if !self.eat(b':') {
                return Err(format!("expected `:` in shape type at offset {}", self.pos));
            }
            self.ty()?;
            self.skip_ws();
            if self.eat(b',') {
                continue;
            }
            if self.eat(b'}') {
                break;
            }
            return Err(format!("expected `,` or `}}` in shape type at offset {}", self.pos));
        }
        Ok(self.postfix_nilable(RbsTy::Simple("untyped".to_string())))
    }

    /// A `[` has already been eaten: comma-separated types through the
    /// closing `]`. Shared by generic args (`Array[T]`) and tuple types
    /// (`[Foo, Bar]`).
    fn bracketed_types(&mut self) -> Result<Vec<RbsTy>, String> {
        self.skip_ws();
        let mut args = vec![self.ty()?];
        self.skip_ws();
        while self.eat(b',') {
            self.skip_ws();
            args.push(self.ty()?);
            self.skip_ws();
        }
        if !self.eat(b']') {
            return Err(format!("expected `]` at offset {}", self.pos));
        }
        Ok(args)
    }

    /// Real RBS nilable syntax is postfix (`Foo?`, sorbet.org/docs/rbs-support's
    /// quick reference table). Prefix `?Foo` (handled in `primary`, above) is
    /// not valid RBS but predates this parser's alignment with the spec —
    /// kept so existing callers passing that shape keep working.
    fn postfix_nilable(&mut self, base: RbsTy) -> RbsTy {
        if self.peek() == Some(b'?') {
            self.pos += 1;
            return RbsTy::Nilable(Box::new(base));
        }
        base
    }
}

/// Target of an inline `#: as <target>` type-assertion comment (bead
/// ita-qst; sorbet.org/docs/rbs-support's "inline type assertion" form —
/// distinct from the method-signature form `parse_rbs_comment` owns
/// above). Seen in the wild trailing a single call argument on its own
/// line, e.g. ruby-lsp's `lib/ruby_indexer/lib/ruby_indexer/index.rb`:
/// `index_single(\n  uri,\n  source, #: as !nil\n)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CastTarget {
    /// `!nil` — strip `Nil` from whatever type the argument already
    /// carries. Resolution (what "whatever type" means, and how `Nil` is
    /// stripped from an `Unknown`) is `check.rs::strip_nil`'s job, not
    /// this pure parser's.
    NotNil,
    /// `untyped` — RBS's dedicated "erase the type entirely" target
    /// (bead ita-j0z). Unlike `Named`, this is NOT resolved through
    /// `sorbet_sig::resolve_ret_ty` at all: erasure is the whole point
    /// of the cast, so the caller must set the argument's type to
    /// `Ty::Unknown` unconditionally, never fall back to the
    /// already-inferred type the way an unresolvable `Named` target
    /// does. Real site: ruby-lsp's settings mixin
    /// (`addon_test.rb:149`) casts a value `as untyped` specifically to
    /// suppress a downstream false positive — a no-op cast there would
    /// defeat the author's intent.
    Untyped,
    /// A bare word naming a target type: a core scalar (`String`,
    /// `Integer`, ...) or a project class path, exactly as spelled.
    /// Resolving it to a `Ty` (and treating an unresolvable name as "no
    /// cast happened", never a guess) is the caller's job — see
    /// `sorbet_sig::resolve_ret_ty`, the same core-scalar/project-class
    /// mapping `sig_fill` already reuses for `.returns(...)` text.
    Named(String),
}

/// Parse the text after `#:` as an inline cast (`as !nil`, `as Foo`,
/// `as Foo::Bar`). Returns `None` for anything else — including a
/// method-signature comment (`(Integer) -> String`), which
/// `parse_rbs_comment` above already owns, and a malformed/garbage cast
/// (`as`, `as Foo Bar`, `asFoo`) — an inline cast that fails to parse is
/// simply not a cast: the caller degrades to whatever the argument's
/// type would have been without this comment at all, never a diagnostic
/// of its own (unlike a malformed method-signature comment, which
/// deliberately DOES raise E0105 — the two forms have different failure
/// contracts on purpose, since a cast comment sits on ordinary code a
/// human is free to leave malformed without meaning to write RBS at
/// all).
///
/// Exactly two whitespace-separated tokens are required (`as` plus the
/// target) — a stray trailing word, or `as` glued to its target with no
/// space (`asFoo`), both fail to parse rather than guessing which part
/// is the real target.
pub fn parse_cast_comment(input: &str) -> Option<CastTarget> {
    // No `trim()`: `split_whitespace` already skips leading and trailing
    // whitespace, and clippy flags the pair as a real redundancy.
    let mut it = input.split_whitespace();
    if it.next() != Some("as") {
        return None;
    }
    let target = it.next()?;
    if it.next().is_some() {
        return None;
    }
    Some(match target {
        "!nil" => CastTarget::NotNil,
        "untyped" => CastTarget::Untyped,
        _ => CastTarget::Named(target.to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn simple(s: &str) -> RbsTy {
        RbsTy::Simple(s.to_string())
    }

    #[test]
    fn basic_sig() {
        let sig = parse_rbs_comment("(Integer) -> String").expect("documented RBS form must parse");
        assert_eq!(sig.params, vec![RbsParam::Required(simple("Integer"))]);
        assert_eq!(sig.ret, simple("String"));
    }

    #[test]
    fn full_sig() {
        let sig = parse_rbs_comment("(Integer, ?String, foo: Symbol) -> Array[String]").expect("documented RBS form must parse");
        assert_eq!(
            sig.params,
            vec![
                RbsParam::Required(simple("Integer")),
                RbsParam::Optional(simple("String")),
                RbsParam::Keyword { name: "foo".into(), ty: simple("Symbol"), required: true },
            ]
        );
        assert_eq!(sig.ret, RbsTy::Generic("Array".into(), vec![simple("String")]));
    }

    #[test]
    fn union_nilable_hash() {
        let sig = parse_rbs_comment("(?Integer, ?bar: bool) -> Hash[Symbol, Integer | nil]").expect("documented RBS form must parse");
        assert_eq!(sig.params.len(), 2);
        assert_eq!(
            sig.ret,
            RbsTy::Generic(
                "Hash".into(),
                vec![simple("Symbol"), RbsTy::Union(vec![simple("Integer"), simple("nil")])]
            )
        );
        let sig2 = parse_rbs_comment("() -> ?String").expect("documented RBS form must parse");
        assert_eq!(sig2.ret, RbsTy::Nilable(Box::new(simple("String"))));
    }

    #[test]
    fn const_path() {
        let sig = parse_rbs_comment("(Foo::Bar) -> self").expect("documented RBS form must parse");
        assert_eq!(sig.params, vec![RbsParam::Required(simple("Foo::Bar"))]);
        assert_eq!(sig.ret, simple("self"));
    }

    #[test]
    fn malformed() {
        assert!(parse_rbs_comment("Integer -> String").is_err());
        assert!(parse_rbs_comment("(Integer -> String").is_err());
        assert!(parse_rbs_comment("(Integer) -> ").is_err());
        assert!(parse_rbs_comment("(Integer) -> String junk").is_err());
    }

    // -- bead ita-p24: forms documented at sorbet.org/docs/rbs-support -----

    #[test]
    fn bare_arrow_no_params() {
        let sig = parse_rbs_comment("-> void").expect("documented RBS form must parse");
        assert_eq!(sig.params, vec![]);
        assert_eq!(sig.ret, simple("void"));
        let sig2 = parse_rbs_comment("-> String?").expect("documented RBS form must parse");
        assert_eq!(sig2.ret, RbsTy::Nilable(Box::new(simple("String"))));
    }

    #[test]
    fn postfix_nilable_type() {
        // The real RBS form (postfix), vs. the prefix form already covered
        // by `union_nilable_hash`'s `-> ?String`.
        let sig = parse_rbs_comment("(String?) -> Symbol?").expect("documented RBS form must parse");
        assert_eq!(sig.params, vec![RbsParam::Required(RbsTy::Nilable(Box::new(simple("String"))))]);
        assert_eq!(sig.ret, RbsTy::Nilable(Box::new(simple("Symbol"))));
    }

    #[test]
    fn positional_parameter_names_are_discarded() {
        // `Integer x` / `?Integer x` — the name is optional and, per this
        // parser's contract, never retained (see `skip_param_name`'s doc).
        let sig = parse_rbs_comment("(Class desired_class, ?String? desired_method, ?id: Integer?) -> untyped")
            .expect("documented RBS form must parse");
        assert_eq!(
            sig.params,
            vec![
                RbsParam::Required(simple("Class")),
                RbsParam::Optional(RbsTy::Nilable(Box::new(simple("String")))),
                RbsParam::Keyword {
                    name: "id".into(),
                    ty: RbsTy::Nilable(Box::new(simple("Integer"))),
                    required: false
                },
            ]
        );
    }

    #[test]
    fn rest_positional_becomes_optional_param() {
        let sig = parse_rbs_comment("(*String args) -> void").expect("documented RBS form must parse");
        assert_eq!(sig.params, vec![RbsParam::Optional(simple("String"))]);
        let sig2 = parse_rbs_comment("(String name, *String more) -> void").expect("documented RBS form must parse");
        assert_eq!(
            sig2.params,
            vec![RbsParam::Required(simple("String")), RbsParam::Optional(simple("String"))]
        );
    }

    #[test]
    fn rest_keyword_is_dropped_not_misnamed() {
        // No fixed name to key a `RbsParam::Keyword` lookup on — must not
        // silently appear as a param at all (dropped, not misrepresented).
        let sig = parse_rbs_comment("(String project_path, **untyped options) -> void").expect("documented RBS form must parse");
        assert_eq!(sig.params, vec![RbsParam::Required(simple("String"))]);
        let sig2 = parse_rbs_comment("(**untyped options) -> void").expect("documented RBS form must parse");
        assert_eq!(sig2.params, vec![]);
    }

    #[test]
    fn block_type_parsed_and_discarded() {
        let sig = parse_rbs_comment("(String module_name) { (Index index, Entry::Namespace base) -> void } -> void")
            .expect("documented RBS form must parse");
        assert_eq!(sig.params, vec![RbsParam::Required(simple("String"))]);
        assert_eq!(sig.ret, simple("void"));

        // Bare block, no leading `(...)` param list at all.
        let sig2 = parse_rbs_comment("{ (Index index, Entry::Namespace base) -> void } -> void").expect("documented RBS form must parse");
        assert_eq!(sig2.params, vec![]);

        // Optional block with `[self: Type]`.
        let sig3 = parse_rbs_comment("(String? query) ?{ (Entry) -> bool? } -> Array[Entry]").expect("documented RBS form must parse");
        assert_eq!(sig3.params, vec![RbsParam::Required(RbsTy::Nilable(Box::new(simple("String"))))]);

        // Block with no params at all: `{ -> T }`.
        let sig4 = parse_rbs_comment("[T] { -> T } -> T").expect("documented RBS form must parse");
        assert_eq!(sig4.params, vec![]);
        assert_eq!(sig4.ret, simple("T"));
    }

    #[test]
    fn qmark_block_params_degrade_to_untyped() {
        // Bead ita-hfn: tapioca's own shape
        // (lib/tapioca/helpers/test/isolation.rb:30,75) — `(?)` as a
        // block's own parameter list, the RBS "untyped function"
        // placeholder (module doc's `(?)` entry). The block's params
        // carry no info; the method's own params/return are untouched.
        let sig = parse_rbs_comment("?{ (?) -> untyped } -> String").expect("tapioca's own RBS form must parse");
        assert_eq!(sig.params, vec![]);
        assert_eq!(sig.ret, simple("String"));

        // Required (non-optional) block, same placeholder.
        let sig2 = parse_rbs_comment("(String path) { (?) -> untyped } -> String")
            .expect("(?) must parse as a required block's param list too");
        assert_eq!(sig2.params, vec![RbsParam::Required(simple("String"))]);

        // Regression: a block with REAL params must keep parsing exactly
        // as before — `(?)` is a new alternative, not a replacement.
        let sig3 = parse_rbs_comment("?{ (String) -> untyped } -> String").expect("real block params must still parse");
        assert_eq!(sig3.ret, simple("String"));
    }

    #[test]
    fn generic_method_type_params_captured() {
        let sig = parse_rbs_comment("[T] (String request_name, T value) -> T").expect("documented RBS form must parse");
        assert_eq!(sig.params, vec![RbsParam::Required(simple("String")), RbsParam::Required(simple("T"))]);
        assert_eq!(sig.ret, simple("T"));
        // Bead ita-s12: the shape rbs_to_ty needs to bind `T` to `Ty::Unknown`.
        assert_eq!(sig.type_params, vec!["T".to_string()]);

        // Multiple type params, and a sig with none at all.
        let sig2 = parse_rbs_comment("[K, V] (K, V) -> Hash[K, V]").expect("documented RBS form must parse");
        assert_eq!(sig2.type_params, vec!["K".to_string(), "V".to_string()]);
        let sig3 = parse_rbs_comment("(Integer) -> String").expect("documented RBS form must parse");
        assert_eq!(sig3.type_params, Vec::<String>::new());

        // Bounded/defaulted params still parse and still capture the name.
        let sig4 = parse_rbs_comment("[T = Array[String]] (T) -> T").expect("documented RBS form must parse");
        assert_eq!(sig4.type_params, vec!["T".to_string()]);
    }

    #[test]
    fn intersection_type_degrades_to_untyped() {
        let sig = parse_rbs_comment("(?Class[(T & Entry)]? type) -> void").expect("documented RBS form must parse");
        assert_eq!(sig.params, vec![RbsParam::Optional(RbsTy::Nilable(Box::new(RbsTy::Generic(
            "Class".into(),
            vec![simple("untyped")]
        ))))]);
    }

    #[test]
    fn proc_type_degrades_to_untyped() {
        let sig = parse_rbs_comment("(^(Integer arg0) -> Integer) -> void").expect("documented RBS form must parse");
        assert_eq!(sig.params, vec![RbsParam::Required(simple("untyped"))]);
        // Zero-arg proc omits its params, exactly like a bare-arrow method sig.
        let sig2 = parse_rbs_comment("(T::Helpers requiring, ^-> void block) -> void").expect("documented RBS form must parse");
        assert_eq!(
            sig2.params,
            vec![RbsParam::Required(simple("T::Helpers")), RbsParam::Required(simple("untyped"))]
        );
    }

    #[test]
    fn shape_type_degrades_to_untyped() {
        let sig = parse_rbs_comment("(Array[{uri: String, type: Integer}] changes) -> void")
            .expect("documented RBS form must parse");
        assert_eq!(
            sig.params,
            vec![RbsParam::Required(RbsTy::Generic("Array".into(), vec![simple("untyped")]))]
        );
        let sig2 = parse_rbs_comment("(String x) -> {}").expect("documented RBS form must parse");
        assert_eq!(sig2.ret, simple("untyped"));
    }

    #[test]
    fn parenthesized_union_return_type() {
        let sig = parse_rbs_comment("(String uri) -> (Array[Entry] | Array[Symbol])?").expect("documented RBS form must parse");
        assert_eq!(
            sig.ret,
            RbsTy::Nilable(Box::new(RbsTy::Union(vec![
                RbsTy::Generic("Array".into(), vec![simple("Entry")]),
                RbsTy::Generic("Array".into(), vec![simple("Symbol")]),
            ])))
        );
    }

    #[test]
    fn tuple_type_degrades_to_untyped() {
        let sig = parse_rbs_comment("(String name) -> [String, String]").expect("documented RBS form must parse");
        assert_eq!(sig.ret, RbsTy::Generic("__tuple".into(), vec![simple("String"), simple("String")]));
        let sig2 = parse_rbs_comment("(String uri) -> [Integer, Integer?]?").expect("documented RBS form must parse");
        assert_eq!(
            sig2.ret,
            RbsTy::Nilable(Box::new(RbsTy::Generic(
                "__tuple".into(),
                vec![simple("Integer"), RbsTy::Nilable(Box::new(simple("Integer")))]
            )))
        );
    }

    #[test]
    fn singleton_type_degrades_to_untyped() {
        let sig = parse_rbs_comment("(String cop_name) -> singleton(::RuboCop::Cop::Base)?")
            .expect("documented RBS form must parse");
        assert_eq!(
            sig.ret,
            RbsTy::Nilable(Box::new(RbsTy::Generic("singleton".into(), vec![simple("::RuboCop::Cop::Base")])))
        );
        let sig2 =
            parse_rbs_comment("(Array[singleton(Prism::Node)] classes) -> void").expect("documented RBS form must parse");
        assert_eq!(
            sig2.params,
            vec![RbsParam::Required(RbsTy::Generic(
                "Array".into(),
                vec![RbsTy::Generic("singleton".into(), vec![simple("Prism::Node")])]
            ))]
        );
    }
    // -- prova dos dois lados: every new accepted shape has a malformed
    // sibling that must still raise E0105. A mutant that accepts input
    // blindly (e.g. `Ok(Default::default())` short-circuiting any of the
    // paths above) is caught by every assertion below.
    #[test]
    fn new_forms_still_reject_malformed_input() {
        // Bare arrow: no `->` at all.
        assert!(parse_rbs_comment("void").is_err());
        // Rest keyword/positional missing their type.
        assert!(parse_rbs_comment("(**) -> void").is_err());
        assert!(parse_rbs_comment("(*) -> void").is_err());
        // Block clause missing its arrow.
        assert!(parse_rbs_comment("{ (String) } -> void").is_err());
        // Block clause never closed.
        assert!(parse_rbs_comment("{ (String) -> void -> void").is_err());
        // `?{` without a following `{`.
        assert!(parse_rbs_comment("(Integer) ?String -> void").is_err());
        // `[self: ...]` with the wrong keyword.
        assert!(parse_rbs_comment("{ (String) [other: Integer] -> void } -> void").is_err());
        // Unterminated generic-method type-parameter list.
        assert!(parse_rbs_comment("[T (Integer) -> void").is_err());
        // Proc type missing its `(`.
        assert!(parse_rbs_comment("(^Integer -> Integer) -> void").is_err());
        // Proc type missing its `->`.
        assert!(parse_rbs_comment("(^(Integer) Integer) -> void").is_err());
        // Intersection with a malformed right-hand side.
        assert!(parse_rbs_comment("(Foo & ) -> void").is_err());
        // Parenthesized type never closed.
        assert!(parse_rbs_comment("(Foo) -> (Bar | Baz").is_err());
        // Tuple type never closed.
        assert!(parse_rbs_comment("(Foo) -> [String, String").is_err());
        // Singleton type missing its `)`.
        assert!(parse_rbs_comment("(String x) -> singleton(Foo").is_err());
        // Singleton type with no type inside the parens.
        assert!(parse_rbs_comment("(String x) -> singleton()").is_err());
        // Shape type missing its `:`.
        assert!(parse_rbs_comment("(String x) -> {uri String}").is_err());
        // Shape type never closed.
        assert!(parse_rbs_comment("(String x) -> {uri: String").is_err());
        // Bead ita-hfn: `(?)` mixed with a real param inside a block — the
        // placeholder is only ever the WHOLE list, never one of several.
        assert!(parse_rbs_comment("?{ (Integer, ?) -> untyped } -> String").is_err());
        // Bead ita-hfn control: `(?)` as the METHOD's OWN top-level param
        // list (not a block) is out of scope for this bead — see module
        // doc's `(?)` entry — and still raises E0105.
        assert!(parse_rbs_comment("(?) -> String").is_err());
    }

    // -- bead ita-qst: inline `#: as <target>` cast comments -----------

    #[test]
    fn cast_not_nil() {
        assert_eq!(parse_cast_comment("as !nil"), Some(CastTarget::NotNil));
    }

    #[test]
    fn cast_named_class() {
        assert_eq!(
            parse_cast_comment("as Foo"),
            Some(CastTarget::Named("Foo".to_string()))
        );
        assert_eq!(
            parse_cast_comment("as Foo::Bar"),
            Some(CastTarget::Named("Foo::Bar".to_string()))
        );
    }

    #[test]
    fn cast_ignores_leading_trailing_whitespace() {
        assert_eq!(parse_cast_comment("  as   Foo  "), Some(CastTarget::Named("Foo".to_string())));
    }

    #[test]
    fn cast_rejects_non_as_forms() {
        // A method-signature comment: owned by `parse_rbs_comment`, not this parser.
        assert_eq!(parse_cast_comment("(Integer) -> String"), None);
        // `as` glued to its target with no space — not a recognized token split.
        assert_eq!(parse_cast_comment("asFoo"), None);
        // Bare `as` with no target at all.
        assert_eq!(parse_cast_comment("as"), None);
        // Trailing junk after the target — never guess which word is the real one.
        assert_eq!(parse_cast_comment("as Foo Bar"), None);
        assert_eq!(parse_cast_comment(""), None);
    }
}
