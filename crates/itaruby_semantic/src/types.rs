//! The type model. Invariant #1 of the whole project: `Unknown` NEVER
//! produces a diagnostic. False negatives are acceptable; false positives
//! are not.

use itaruby_syntax::SourceFile;

/// Index into `ProjectIndex::classes`. Only meaningful together with the
/// `ProjectIndex` it came from.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ClassId(pub u32);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Ty {
    /// Not inferable. Absorbs everything; suppresses all diagnostics.
    Unknown,
    Nil,
    Bool,
    Int,
    Float,
    Str,
    Sym,
    /// Instance of a project class.
    Instance(ClassId),
    /// The class object itself (`Foo` in expression position).
    Class(ClassId),
    Array(Box<Ty>),
    Hash(Box<Ty>, Box<Ty>),
    Union(Vec<Ty>),
}

impl Ty {
    /// Union of two types, flattened and deduped. Unknown absorbs.
    ///
    /// # Panics
    ///
    /// Never panics: the final `unwrap()` only runs when `uniq.len() == 1`,
    /// which guarantees `pop()` returns `Some`.
    pub fn union(a: Ty, b: Ty) -> Ty {
        if a == b {
            return a;
        }
        if a == Ty::Unknown || b == Ty::Unknown {
            return Ty::Unknown;
        }
        let mut parts: Vec<Ty> = Vec::new();
        for t in [a, b] {
            match t {
                Ty::Union(ts) => parts.extend(ts),
                t => parts.push(t),
            }
        }
        parts.dedup();
        let mut uniq: Vec<Ty> = Vec::new();
        for t in parts {
            if !uniq.contains(&t) {
                uniq.push(t);
            }
        }
        match uniq.len() {
            // `pop` cannot come back empty on this arm. The fallback says so
            // without a panic, and it picks the one value that is always safe
            // here: `Unknown` is this checker's silence, so even an
            // unreachable path stays inside invariant #1.
            1 => uniq.pop().unwrap_or(Ty::Unknown),
            _ => Ty::Union(uniq),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Severity {
    Error,
    Warning,
}

/// Did-you-mean data attached to a diagnostic for rendering (w12
/// closure): the closest known spelling of the typo'd symbol within edit
/// distance 2 (ties broken lexicographically), among the names the
/// checker already knew at the error site. `def` is where the index says
/// the suggested symbol is defined (file + byte offset of its name),
/// when the index tracks one — method suggestions can have it, constant
/// suggestions never do (the index stores no def site for constants), and
/// it is never guessed. Pure payload: it never influences whether a
/// diagnostic fires, its code, severity, or message.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Suggestion {
    pub name: String,
    pub def: Option<(SourceFile, usize)>,
}

/// An owned, per-file diagnostic. Positions are byte offsets into the file.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Diagnostic {
    pub start: usize,
    pub end: usize,
    pub code: &'static str,
    pub severity: Severity,
    pub message: String,
    pub suggestion: Option<Suggestion>,
    /// Proof payload for E0107 (bead ita-dqo): `Some` only on that one
    /// code, `None` on every other diagnostic. Pure rendering payload,
    /// exactly like `suggestion` — it never influences whether a
    /// diagnostic fires, its code, severity, or message.
    pub constraint: Option<ConstraintProof>,
}

/// One usage constraint on an Unknown receiver binding: "responds to
/// `method`". Pure payload (like `Suggestion`): never influences whether
/// a diagnostic fires.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ConstraintCall {
    pub method: String,
    /// Call-site span (byte offsets, message location).
    pub start: usize,
    pub end: usize,
    /// Sorted, deduped display names of every closed-ancestry candidate
    /// defining `method`: project class paths and core class names.
    pub candidates: Vec<String>,
}

/// Proof behind an E0107: >= 2 constraint calls on the same local
/// binding whose candidate sets have an empty intersection.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ConstraintProof {
    /// Local variable / parameter name of the receiver.
    pub receiver: String,
    /// Ordered by first call-site offset; every entry has >= 1 candidate.
    pub calls: Vec<ConstraintCall>,
}

pub const E0001_SYNTAX_ERROR: &str = "E0001";
pub const E0101_UNKNOWN_METHOD: &str = "E0101";
pub const E0102_WRONG_ARITY: &str = "E0102";
pub const E0103_ARG_TYPE_MISMATCH: &str = "E0103";
pub const E0104_UNRESOLVED_CONSTANT: &str = "E0104";
pub const E0105_INVALID_RBS_COMMENT: &str = "E0105";
/// Column type declared in `db/schema.rb` cannot survive Rails' cast for a
/// literal argument (bead ita-yho): non-numeric `String` into an
/// integer/bigint/decimal column, or a digit-less `String` into a
/// date/datetime column. Warning, never Error — the cast is permissive by
/// design (`"42"` into an integer column casts fine and stays silent).
pub const E0106_IMPOSSIBLE_CAST: &str = "E0106";
/// Constraint contradiction (bead ita-dqo): >= 2 distinct method calls on
/// the same Unknown local-var/parameter binding whose closed-ancestry
/// candidate sets (project classes ∪ core classes) have an empty
/// intersection — no type in the project or the core library responds to
/// every method the binding was called with. Always `Severity::Warning`,
/// never `Error`: it never affects exit code, and — unlike E0101-E0106 —
/// it only fires when `closed_world()` is on (see `check.rs`'s
/// `Checker::flush_constraints`), so it is impossible under `ita
/// server`/LSP by construction.
pub const E0107_CONSTRAINT_CONTRADICTION: &str = "E0107";
/// Operator operand type mismatch: an arithmetic/concatenation operator
/// whose BOTH operands are proven from literals inside one scope, in a
/// pairing MRI raises `TypeError` on (`100 + "x"`, `"x" + 100`,
/// `1.5 - nil`, ...). `Severity::Error` — the program really crashes on
/// that line. The proof is doubly conservative (see `check.rs`'s
/// `Checker::check_operand_types`): flow-sensitively the walker's env
/// must already know the type AND flow-insensitively every write to
/// that local inside the same scope must be a literal of the same type,
/// so one reassignment anywhere silences the site. Never fires when the
/// receiver's or the operand's core class is reopened by the project or
/// by any declaration — a user-defined `coerce`/`+` makes the pairing
/// legal at runtime.
pub const E0108_OPERAND_TYPE_MISMATCH: &str = "E0108";
/// A proven source-method return contradicts its explicit Sorbet contract.
/// Unknown, void, and declaration-only bodies never emit this diagnostic.
pub const E0109_RETURN_TYPE_MISMATCH: &str = "E0109";
