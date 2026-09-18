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
#[test]
fn the_class_object_surface_is_excluded() {
    for banned in [
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
    ] {
        let leaked: Vec<&str> = pairs()
            .iter()
            .filter(|(_, m)| *m == banned)
            .map(|(ns, _)| *ns)
            .take(3)
            .collect();
        assert!(
            leaked.is_empty(),
            "`{banned}` is Module/Class surface and must not be in the inventory (e.g. {leaked:?})"
        );
    }
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
