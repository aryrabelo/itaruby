//! Singleton-track family (e): the generated stdlib singleton inventory
//! (`declarations/stdlib_singletons.txt`, produced by
//! `scripts/gen-stdlib-singleton-inventory.rb`).
//!
//! WHY THESE ARE TESTS AND NOT A README SENTENCE. The generator excludes
//! two things by construction: the class-object surface every
//! `Module`/`Class` answers (which `core.rs::kernel_object_singleton_method`
//! already models) and the ten core classes `core_inventory.txt` owns.
//! Both exclusions live in the generator, which means they hold only for
//! as long as whoever regenerates the file keeps them — and AGENTS.md's
//! own measured lesson is that a filter proven by reading is not proven
//! (a gem in the collection can be "filtered upstream" and still yield
//! entries; confirm the count, not the intent). So the properties are
//! asserted against the COMMITTED file: a regeneration that widens the
//! inventory into either excluded territory fails the build.
//!
//! The file itself is hash-locked by `L2.GENERATED_FILES_ARE_LOCKED`, so
//! hand-editing it to make these pass is not a path either.

const TXT: &str = include_str!("../declarations/stdlib_singletons.txt");

fn pairs() -> Vec<(&'static str, &'static str)> {
    TXT.lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
        .filter_map(|l| l.rsplit_once('.'))
        .collect()
}

/// The inventory answers the receivers the residue probe actually found:
/// `FileUtils.*` is 270 of discourse's explicit-receiver residue sites,
/// and `SecureRandom.uuid`/`Kernel.rand` are the two names
/// `singleton_lookup.rs`'s header has cited as the reason the singleton
/// arm stays silent since the gap was first measured.
#[test]
fn the_inventory_covers_the_measured_residue_receivers() {
    for want in [
        "FileUtils.mkdir_p",
        "FileUtils.rm_rf",
        "FileUtils.mv",
        "FileUtils.cp",
        "SecureRandom.uuid",
        "SecureRandom.hex",
        "Kernel.rand",
        "Kernel.raise",
        "Kernel.require",
        "Kernel.Array",
        "Kernel.Integer",
        "Kernel.block_given?",
        "Math.sqrt",
        "Process.pid",
        "JSON.parse",
        "Digest::MD5.hexdigest",
    ] {
        assert!(
            TXT.lines().any(|l| l == want),
            "{want} missing from the generated stdlib singleton inventory"
        );
    }
}

/// EXCLUSION 1, and the reason `singleton_methods(false)` was not enough:
/// the harvest is `singleton_methods(true)` (so `extend`-provided module
/// functions like `SecureRandom.uuid` are seen) MINUS everything
/// `Module`/`Class` answer. If a regeneration loses that subtraction,
/// every namespace in the file grows ~100 entries of class-object
/// surface — which `core.rs::kernel_object_singleton_method` already
/// covers, and which would make "what does this file add?" unanswerable.
///
/// This used to assert a TEN-NAME SAMPLE, which is an instrument that
/// passes while most of the subtraction is gone: a regeneration that
/// lost every name but those ten would have been reported clean. The
/// banned set is now the WHOLE surface, computed from the same
/// expression the generator subtracts
/// (`scripts/gen-stdlib-singleton-inventory.rb`'s
/// `CLASS_OBJECT_SURFACE`) plus the module objects whose surface rides
/// along on it — around 130 names instead of 10, `new` and `allocate`
/// among them.
///
/// Two-sided in the same test, because a computed banned set can fail
/// open in three ways and each one is checked: the list must be
/// substantial, it must still contain the ten names this assertion
/// shipped with, and the same predicate must FLAG a synthetic leaked
/// pair. Without `ruby` on PATH the ten-name floor is used instead —
/// weaker, never vacuous.
#[test]
fn the_class_object_surface_is_excluded() {
    // The names this test asserted before the surface was computed.
    // They are the floor: whatever else changes, these stay banned.
    const SAMPLE: [&str; 10] = [
        "name",
        "ancestors",
        "instance_methods",
        "const_get",
        "class_eval",
        "module_eval",
        "define_method",
        "superclass",
        "allocate",
        "instance_variable_get",
    ];
    let banned = class_object_surface().unwrap_or_else(|| {
        eprintln!("no usable `ruby` on PATH — falling back to the ten-name floor");
        SAMPLE.iter().map(|s| (*s).to_string()).collect()
    });
    for floor in SAMPLE {
        assert!(
            banned.iter().any(|b| b == floor),
            "`{floor}` vanished from the computed class-object surface — the \
             expression that produces it no longer describes what it used to"
        );
    }
    let leaked = leaked_pairs(&banned, &pairs());
    assert!(
        leaked.is_empty(),
        "Module/Class surface must not be in the inventory: {:?} (+{} more)",
        leaked.iter().take(5).collect::<Vec<_>>(),
        leaked.len().saturating_sub(5)
    );
    // POSITIVE CONTROL: the predicate that just answered "clean" must
    // answer "leaked" on a planted pair, or its silence proves nothing.
    let mut planted = pairs();
    planted.push(("PlantedNamespace", "name"));
    assert_eq!(
        leaked_pairs(&banned, &planted),
        vec!["PlantedNamespace.name".to_string()],
        "the leak check is blind: it did not flag a planted class-object name"
    );
}

/// Every `<namespace>.<method>` pair whose method is in `banned`.
fn leaked_pairs(banned: &[String], pairs: &[(&str, &str)]) -> Vec<String> {
    pairs
        .iter()
        .filter(|(_, m)| banned.iter().any(|b| b == m))
        .map(|(ns, m)| format!("{ns}.{m}"))
        .collect()
}

/// Everything a class object answers to, from MRI itself: the
/// generator's own `Module.methods | Class.methods`, plus
/// `BasicObject`/`Comparable`/`Enumerable` — the three whose own class
/// objects the inventory walks — so the banned set is the surface as a
/// whole and not a reading of it. `None` when `ruby` cannot be run or
/// answers with something implausible.
fn class_object_surface() -> Option<Vec<String>> {
    let out = std::process::Command::new("ruby")
        .arg("-e")
        .arg(
            "puts (Module.methods | Class.methods | BasicObject.methods | \
             Comparable.methods | Enumerable.methods).map(&:to_s).uniq.sort",
        )
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let names: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    // A truncated or empty answer must not read as "nothing is banned".
    (names.len() >= 50).then_some(names)
}

/// EXCLUSION 2: the ten core classes `declarations/core_inventory.txt`
/// owns. Two generated files claiming one namespace is how a filter
/// rots — and `core_inventory.txt` is the one the type-level core model
/// reads.
#[test]
fn the_core_classes_are_left_to_core_inventory() {
    for owned in [
        "Integer", "Float", "String", "Symbol", "Array", "Hash", "NilClass", "TrueClass",
        "FalseClass", "Object",
    ] {
        let leaked: Vec<&str> = pairs()
            .iter()
            .filter(|(ns, _)| *ns == owned)
            .map(|(_, m)| *m)
            .take(3)
            .collect();
        assert!(
            leaked.is_empty(),
            "`{owned}` belongs to core_inventory.txt, found {leaked:?} here"
        );
    }
}

/// Shape, so a malformed regeneration is loud rather than silently
/// half-loaded: every non-comment line is exactly `Namespace.method`,
/// with a constant-looking namespace and no tab column (this file has no
/// lib attribution, by design — see its header).
#[test]
fn every_line_is_a_namespace_dot_method_pair() {
    let mut count = 0usize;
    for line in TXT.lines().filter(|l| !l.starts_with('#') && !l.is_empty()) {
        assert!(!line.contains('\t'), "unexpected lib column: {line}");
        let (ns, m) = line.rsplit_once('.').unwrap_or_else(|| panic!("not a pair: {line}"));
        assert!(
            ns.starts_with(|c: char| c.is_ascii_uppercase()),
            "namespace must look like a constant: {line}"
        );
        assert!(!m.is_empty(), "empty method name: {line}");
        count += 1;
    }
    assert!(count > 5_000, "inventory suspiciously small: {count} pairs");
}

/// Sorted and deduplicated, which is what makes a regeneration's diff
/// readable — the only way anyone will ever audit a 7000-line generated
/// file is by reading the diff.
#[test]
fn the_inventory_is_sorted_and_unique() {
    let lines: Vec<&str> =
        TXT.lines().filter(|l| !l.starts_with('#') && !l.is_empty()).collect();
    let mut sorted = lines.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(lines, sorted, "inventory must be sorted and unique");
}

/// The CONSUMER, two-sided. `soften_not_found` asks exactly this
/// question on the singleton track, and both answers matter: a hit is
/// the silence family (e) exists to produce, and a miss on a near-name
/// (`mkdir_pp`, `uuidd`) is what will let E0101 report on this track
/// once the residue reaches zero. A predicate that answered `true` for
/// everything would pass every other test in this file.
#[test]
fn the_predicate_answers_both_ways() {
    for (ns, m) in [
        ("FileUtils", "mkdir_p"),
        ("::FileUtils", "mkdir_p"),
        ("SecureRandom", "uuid"),
        ("Kernel", "rand"),
        ("Digest::MD5", "hexdigest"),
    ] {
        assert!(itaruby_semantic::stdlib_singleton_method(ns, m), "{ns}.{m} must be known");
    }
    for (ns, m) in [
        // near-misses on a real namespace: the typo signal
        ("FileUtils", "mkdir_pp"),
        ("SecureRandom", "uuidd"),
        ("Kernel", "randd"),
        // a namespace the stdlib does not own at all
        ("Workspace", "prepare"),
        ("FileUtils::Nope", "mkdir_p"),
        // instance-track names must not leak in through this door
        ("FileUtils", "each_with_index"),
    ] {
        assert!(!itaruby_semantic::stdlib_singleton_method(ns, m), "{ns}.{m} must NOT be known");
    }
}
