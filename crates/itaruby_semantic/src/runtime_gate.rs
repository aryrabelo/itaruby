//! Project-wide facts that leave sorbet-runtime's enforcement of a sig
//! unproven (r4-runtime-gates). Each one makes EVERY sig in the project
//! inert (`ProjectIndex::sorbet_runtime_unchecked`): fail-closed and
//! project-wide, never tracked per class, because each can reach any
//! class through a spelling this checker does not follow.
//!
//! * A reference to `T::Configuration` or `T::Private`, in any spelling
//!   (a constant path, a constant read inside `module T`, `T.const_get`,
//!   a string or symbol naming it), other than the literal statement
//!   `T::Configuration.default_checked_level = :always`.
//! * A definition of a method named `sig`, on any receiver.
//! * A definition of `method_added`/`singleton_method_added` whose body
//!   does not call `super`; a definer-call or `alias` spelling of either
//!   counts whatever it runs.
//! * Outside the checked files only, a definition of a type test
//!   (`index::TYPE_TEST_NAMES`): inside them the checker bounds it by the
//!   class it lands on (`check.rs`'s `type_test_unpatched`).
//!
//! `index.rs` reads the facts from each checked file; `discovery.rs`
//! reads them from every other Ruby file under the project root.

use itaruby_syntax::ruby_prism::{self, Node, Visit};

use crate::index::{const_path_str, TYPE_TEST_NAMES};

/// The hooks sorbet-runtime attaches a sig from.
const SIG_HOOKS: &[&[u8]] = &[b"method_added", b"singleton_method_added"];

/// Calls that define a method named by a literal argument.
const DEFINERS: &[&[u8]] = &[
    b"define_method",
    b"define_singleton_method",
    b"alias_method",
    b"attr",
    b"attr_reader",
    b"attr_accessor",
    b"delegate",
    b"def_delegator",
    b"def_delegators",
    b"def_instance_delegator",
    b"def_instance_delegators",
    b"def_single_delegator",
    b"def_single_delegators",
];

/// Does this file's text carry a hazard? `type_tests` also counts a type
/// test definition (a file outside the checked set).
pub fn sorbet_runtime_hazard(text: &str, type_tests: bool) -> bool {
    mentions_hazard(text, type_tests) && tree_hazard(&ruby_prism::parse(text.as_bytes()).node(), type_tests)
}

/// Every hazard spells one of these words, so a file with none of them is
/// never parsed for it.
pub(crate) fn mentions_hazard(text: &str, type_tests: bool) -> bool {
    ["Configuration", "Private", "sig", "method_added"].iter().any(|word| text.contains(word))
        || (type_tests && TYPE_TEST_NAMES.iter().any(|name| text.contains(name)))
}

/// Does this parsed tree carry a hazard?
pub(crate) fn tree_hazard(node: &Node<'_>, type_tests: bool) -> bool {
    let mut scan = GateScan { hazard: false, type_tests, t_depth: 0 };
    scan.visit(node);
    scan.hazard
}

struct GateScan {
    hazard: bool,
    type_tests: bool,
    /// How many `module T` bodies enclose the node: a bare
    /// `Configuration` there is `T::Configuration`.
    t_depth: usize,
}

impl GateScan {
    /// A definition of `name` that sorbet-runtime cannot survive.
    fn defines(&mut self, name: &[u8]) {
        self.hazard |= name == b"sig"
            || SIG_HOOKS.contains(&name)
            || (self.type_tests && TYPE_TEST_NAMES.iter().any(|test| test.as_bytes() == name));
    }
}

impl<'pr> Visit<'pr> for GateScan {
    fn visit_constant_path_node(&mut self, node: &ruby_prism::ConstantPathNode<'pr>) {
        let runtime = node.name().is_some_and(|name| runtime_namespace(name.as_slice()));
        self.hazard |= runtime && node.parent().as_ref().is_some_and(names_t);
        ruby_prism::visit_constant_path_node(self, node);
    }

    fn visit_constant_read_node(&mut self, node: &ruby_prism::ConstantReadNode<'pr>) {
        self.hazard |= self.t_depth > 0 && runtime_namespace(node.name().as_slice());
    }

    fn visit_module_node(&mut self, node: &ruby_prism::ModuleNode<'pr>) {
        let t = usize::from(names_t(&node.constant_path()));
        self.t_depth += t;
        ruby_prism::visit_module_node(self, node);
        self.t_depth -= t;
    }

    fn visit_call_node(&mut self, node: &ruby_prism::CallNode<'pr>) {
        if always_checked_statement(node) {
            return;
        }
        let name = node.name();
        self.hazard |= name.as_slice() == b"const_get" && node.receiver().as_ref().is_some_and(names_t);
        if DEFINERS.contains(&name.as_slice()) {
            let args: Vec<Node<'_>> = node.arguments().map(|a| a.arguments().iter().collect()).unwrap_or_default();
            for literal in args.iter().filter_map(literal_name) {
                self.defines(&literal);
            }
        }
        ruby_prism::visit_call_node(self, node);
    }

    fn visit_string_node(&mut self, node: &ruby_prism::StringNode<'pr>) {
        self.hazard |= names_runtime_namespace(node.unescaped());
    }

    fn visit_symbol_node(&mut self, node: &ruby_prism::SymbolNode<'pr>) {
        self.hazard |= names_runtime_namespace(node.unescaped());
    }

    fn visit_def_node(&mut self, node: &ruby_prism::DefNode<'pr>) {
        let name = node.name();
        if SIG_HOOKS.contains(&name.as_slice()) {
            let mut calls = SuperCallScan(false);
            if let Some(body) = node.body() {
                calls.visit(&body);
            }
            self.hazard |= !calls.0;
        } else {
            self.defines(name.as_slice());
        }
        ruby_prism::visit_def_node(self, node);
    }

    fn visit_alias_method_node(&mut self, node: &ruby_prism::AliasMethodNode<'pr>) {
        if let Some(literal) = literal_name(&node.new_name()) {
            self.defines(&literal);
        }
        ruby_prism::visit_alias_method_node(self, node);
    }
}

/// `Configuration` or `Private`: the names under `T` that reach
/// sorbet-runtime's switches.
fn runtime_namespace(name: &[u8]) -> bool {
    matches!(name, b"Configuration" | b"Private")
}

/// A string naming `T::Configuration` or `T::Private` (a `const_get`
/// argument, an `eval` body).
fn names_runtime_namespace(text: &[u8]) -> bool {
    [b"T::Configuration".as_slice(), b"T::Private".as_slice()]
        .iter()
        .any(|needle| text.windows(needle.len()).any(|w| w == *needle))
}

/// A constant whose last segment is `T` (`T`, `::T`, `A::T`).
fn names_t(node: &Node<'_>) -> bool {
    const_path_str(node).is_some_and(|path| path.rsplit("::").next() == Some("T"))
}

/// The one statement that tightens rather than softens enforcement:
/// `T::Configuration.default_checked_level = :always`, literally.
fn always_checked_statement(node: &ruby_prism::CallNode<'_>) -> bool {
    let receiver = node.receiver().as_ref().and_then(const_path_str);
    let args: Vec<Node<'_>> = node.arguments().map(|a| a.arguments().iter().collect()).unwrap_or_default();
    node.name().as_slice() == b"default_checked_level="
        && !node.is_safe_navigation()
        && node.block().is_none()
        && receiver.is_some_and(|r| r.trim_start_matches("::") == "T::Configuration")
        && matches!(args.as_slice(), [value] if value.as_symbol_node().is_some_and(|s| s.unescaped() == b"always"))
}

/// A symbol or string literal's text.
fn literal_name(node: &Node<'_>) -> Option<Vec<u8>> {
    node.as_symbol_node()
        .map(|sym| sym.unescaped().to_vec())
        .or_else(|| node.as_string_node().map(|s| s.unescaped().to_vec()))
}

/// Does a body contain a `super` / bare `super` call anywhere?
struct SuperCallScan(bool);

impl<'pr> Visit<'pr> for SuperCallScan {
    fn visit_super_node(&mut self, _node: &ruby_prism::SuperNode<'pr>) {
        self.0 = true;
    }

    fn visit_forwarding_super_node(&mut self, _node: &ruby_prism::ForwardingSuperNode<'pr>) {
        self.0 = true;
    }
}
