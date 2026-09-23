//! `db/schema.rb` as an external type declaration for `ActiveRecord`
//! attributes (bead ita-yho). Parsed with prism — schema.rb is plain,
//! dumper-generated Ruby, not hand-written DSL (measured on the reference
//! corpus: zero `t.references`, zero `t.timestamps`). Same
//! "external-declaration-refines-a-project-type" shape as `rbs_comment.rs`:
//! a column whose type this module doesn't recognize degrades to `untyped`
//! (checker treats it as `Ty::Unknown`), never an error — invariant #1.
//!
//! `apply_schema_attributes` runs as a second pass over an already-merged
//! `ProjectIndex`, so an explicit `def age` (or `attr_accessor`) in the
//! model file always wins over the schema guess: `HashMap::entry` only
//! fills in what a file didn't already define.

use std::collections::HashMap;

use itaruby_syntax::ruby_prism::{self, Node, Visit};
use itaruby_syntax::SourceFile;

use crate::index::{MethodSig, ProjectIndex, TableNameDecl};
use crate::rbs_comment::RbsSig;

/// One column as declared by `t.<type> "name"` / `t.column "name", :type`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchemaColumn {
    /// Raw type name as written in the schema (`"integer"`, `"jsonb"`, ...).
    pub type_name: String,
    pub span: (usize, usize),
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct SchemaTable {
    pub columns: HashMap<String, SchemaColumn>,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct SchemaIndex {
    pub tables: HashMap<String, SchemaTable>,
}

/// Column type names this module recognizes as real `create_table` columns.
/// Anything else seen as `t.<name>` (`t.index`, `t.timestamps`,
/// `t.references`, `t.foreign_key`, ...) is not a column and is skipped —
/// this doubles as the "ignore t.index" rule from the bead contract: an
/// unrecognized `t.<name>` never becomes a phantom column.
const KNOWN_COLUMN_TYPES: &[&str] = &[
    "string", "text", "integer", "bigint", "boolean", "decimal", "date", "datetime", "json",
    "jsonb", "inet",
];

/// Parse one `db/schema.rb` file. Depends only on this file's text (mirrors
/// `file_defs`'s incrementality note): editing a model doesn't reparse the
/// schema, editing the schema doesn't reparse every model.
#[salsa::tracked]
pub fn schema_of_file(db: &dyn salsa::Database, file: SourceFile) -> SchemaIndex {
    parse_schema_text(file.text(db))
}

/// Pure parse, independent of salsa/`SourceFile` — the unit-testable core.
/// Unparseable or `create_table`-less input degrades to an empty index,
/// never panics: prism itself is error-tolerant (see `check_file`'s own use
/// of `parse.errors()` instead of failing), and a walk that finds nothing
/// simply inserts nothing.
pub fn parse_schema_text(text: &str) -> SchemaIndex {
    let parse = ruby_prism::parse(text.as_bytes());
    let mut collector = SchemaCollector { tables: HashMap::new() };
    collector.visit(&parse.node());
    SchemaIndex { tables: collector.tables }
}

struct SchemaCollector {
    tables: HashMap<String, SchemaTable>,
}

impl<'pr> Visit<'pr> for SchemaCollector {
    fn visit_call_node(&mut self, node: &ruby_prism::CallNode<'pr>) {
        if node.name().as_slice() == b"create_table" {
            self.collect_create_table(node);
        }
        ruby_prism::visit_call_node(self, node);
    }
}

impl SchemaCollector {
    fn collect_create_table(&mut self, call: &ruby_prism::CallNode<'_>) {
        let Some(args) = call.arguments() else { return };
        let arg_nodes: Vec<Node<'_>> = args.arguments().iter().collect();
        let Some(table_name_node) = arg_nodes.first().and_then(Node::as_string_node) else { return };
        let table_name = String::from_utf8_lossy(table_name_node.unescaped()).into_owned();

        let id_false = arg_nodes.iter().any(|a| {
            a.as_keyword_hash_node().is_some_and(|kw| {
                kw.elements().iter().any(|el| {
                    el.as_assoc_node().is_some_and(|assoc| {
                        is_symbol(&assoc.key(), "id") && assoc.value().as_false_node().is_some()
                    })
                })
            })
        });

        let Some(block) = call.block().and_then(|b| b.as_block_node()) else { return };
        let Some(body) = block.body() else { return };

        let mut table = SchemaTable::default();
        if !id_false {
            // ponytail: real Rails also allows `id: :uuid` etc.; the
            // reference corpus never uses anything but the default bigint
            // primary key, so every non-`id: false` table gets an implicit
            // `id: Integer` column per the bead contract. Upgrade if a
            // corpus finding needs a non-integer primary key modeled.
            table.columns.insert(
                "id".to_string(),
                SchemaColumn { type_name: "integer".to_string(), span: span_of(&call.as_node()) },
            );
        }
        walk_block_body(&body, &mut table);
        // A `create_table` for a name already seen (unusual — schema.rb is
        // a straight dump, never hand-edited) replaces rather than merges:
        // not worth the complexity for a case the corpus doesn't have.
        self.tables.insert(table_name, table);
    }
}

fn walk_block_body(node: &Node<'_>, table: &mut SchemaTable) {
    if let Some(stmts) = node.as_statements_node() {
        for stmt in &stmts.body() {
            collect_column(&stmt, table);
        }
    } else if let Some(begin) = node.as_begin_node() {
        if let Some(stmts) = begin.statements() {
            walk_block_body(&stmts.as_node(), table);
        }
    } else {
        collect_column(node, table);
    }
}

fn collect_column(node: &Node<'_>, table: &mut SchemaTable) {
    let Some(call) = node.as_call_node() else { return };
    if call.receiver().is_none() {
        return;
    }
    let name = String::from_utf8_lossy(call.name().as_slice()).into_owned();
    let Some(args) = call.arguments() else { return };
    let arg_nodes: Vec<Node<'_>> = args.arguments().iter().collect();

    if name == "column" {
        let Some(col) = arg_nodes.first().and_then(Node::as_string_node) else { return };
        let Some(ty) = arg_nodes.get(1).and_then(Node::as_symbol_node) else { return };
        let col_name = String::from_utf8_lossy(col.unescaped()).into_owned();
        let type_name = String::from_utf8_lossy(ty.unescaped()).into_owned();
        table.columns.insert(col_name, SchemaColumn { type_name, span: span_of(node) });
        return;
    }

    if !KNOWN_COLUMN_TYPES.contains(&name.as_str()) {
        return;
    }
    for arg in &arg_nodes {
        let Some(s) = arg.as_string_node() else { continue };
        let col_name = String::from_utf8_lossy(s.unescaped()).into_owned();
        table
            .columns
            .insert(col_name, SchemaColumn { type_name: name.clone(), span: span_of(node) });
    }
}

fn is_symbol(node: &Node<'_>, expected: &str) -> bool {
    node.as_symbol_node()
        .is_some_and(|s| String::from_utf8_lossy(s.unescaped()) == expected)
}

fn span_of(node: &Node<'_>) -> (usize, usize) {
    let loc = node.location();
    (loc.start_offset(), loc.end_offset())
}

// ---------------------------------------------------------------------------
// Type mapping: raw schema type name -> checker types.
// ---------------------------------------------------------------------------

/// Which E0106 rule, if any, applies to a column of this declared type.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CastRisk {
    /// integer/bigint/decimal: a non-numeric literal `String` is senseless.
    Numeric,
    /// date/datetime: a literal `String` with no digit at all is senseless.
    Temporal,
}

pub fn cast_risk(type_name: &str) -> Option<CastRisk> {
    match type_name {
        "integer" | "bigint" | "decimal" => Some(CastRisk::Numeric),
        "date" | "datetime" => Some(CastRisk::Temporal),
        _ => None,
    }
}

/// The attribute reader's declared type, expressed as `RbsTy` so it flows
/// through the exact same `rbs_to_ty` conversion an inline `#:` sig uses
/// (see `check.rs`) — one declaration-refines-type pipeline, not two.
///
/// `decimal`/`date`/`datetime`/`json`/`jsonb`/`inet`/anything unrecognized
/// maps to `untyped`: no `Ty::BigDecimal`/`Date`/`Time` variant exists in
/// this checker, and `core.rs` has no method table for them either.
/// ponytail: the ceiling is "these columns read back as Unknown, so chained
/// calls on them are never checked"; upgrade is adding the `Ty` variants
/// plus core method tables if a corpus finding ever needs it — invariant #1
/// makes the simplification safe today (false negative, never a false
/// positive).
pub fn col_rbs_ty(type_name: &str) -> crate::rbs_comment::RbsTy {
    use crate::rbs_comment::RbsTy;
    match type_name {
        "string" | "text" => RbsTy::Simple("String".to_string()),
        "integer" | "bigint" => RbsTy::Simple("Integer".to_string()),
        "boolean" => RbsTy::Simple("bool".to_string()),
        _ => RbsTy::Simple("untyped".to_string()),
    }
}

// ---------------------------------------------------------------------------
// Model -> table convention.
// ---------------------------------------------------------------------------

/// `snake_case` + simple English pluralization of a class's last path
/// segment, e.g. `Shop::BlogPost` -> `blog_posts`. Deliberately naive (no
/// irregular-noun dictionary — "simple English pluralization" per the bead
/// contract): a wrong guess just fails to match any real table name, which
/// `apply_schema_attributes` already treats as silence.
pub fn derive_table_name(class_path: &str) -> String {
    let simple = class_path.rsplit("::").next().unwrap_or(class_path);
    pluralize(&snake_case(simple))
}

fn snake_case(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    let mut out = String::with_capacity(name.len() + 4);
    for (i, &c) in chars.iter().enumerate() {
        if c.is_uppercase() {
            let prev_lower_or_digit =
                i > 0 && (chars[i - 1].is_lowercase() || chars[i - 1].is_ascii_digit());
            let next_lower = chars.get(i + 1).is_some_and(|c| c.is_lowercase());
            if i > 0 && (prev_lower_or_digit || next_lower) {
                out.push('_');
            }
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

fn pluralize(word: &str) -> String {
    let bytes = word.as_bytes();
    let ends_with = |suffix: &str| word.ends_with(suffix);
    if ends_with("y") && bytes.len() >= 2 && !matches!(bytes[bytes.len() - 2], b'a' | b'e' | b'i' | b'o' | b'u') {
        format!("{}ies", &word[..word.len() - 1])
    } else if ends_with("s") || ends_with("x") || ends_with("z") || ends_with("ch") || ends_with("sh") {
        format!("{word}es")
    } else {
        format!("{word}s")
    }
}

// ---------------------------------------------------------------------------
// Attaching synthetic attributes to the project index.
// ---------------------------------------------------------------------------

/// Second pass over an already-merged `ProjectIndex`: for every
/// `ApplicationRecord`/`ActiveRecord::Base` model whose table resolves in
/// `schema`, synthesize a reader + writer per column. Mirrors the
/// `attr_reader`/`attr_writer` synthesis in `index.rs`'s `DefWalker`, except
/// the declaration lives in a different file (`db/schema.rb`), so it has to
/// run after every file's own methods are already merged:
/// `HashMap::entry().or_insert_with` never overwrites a method the model
/// itself defined.
///
/// Every silence path from the bead contract lives here: dynamic
/// `self.table_name` (`TableNameDecl::Dynamic`), a derived/declared table
/// name absent from `schema`, and a namespaced/non-AR class — all just
/// `continue`, no diagnostic, ever.
pub fn apply_schema_attributes(index: &mut ProjectIndex, schema: &SchemaIndex, schema_file: SourceFile) {
    if schema.tables.is_empty() {
        return;
    }
    for class in &mut index.classes {
        if class.is_module {
            continue;
        }
        let is_ar_model = matches!(
            class.superclass.as_deref(),
            Some("ApplicationRecord" | "ActiveRecord::Base")
        );
        if !is_ar_model {
            continue;
        }
        let table_name = match &class.table_name {
            Some(TableNameDecl::Dynamic) => continue,
            Some(TableNameDecl::Literal(name)) => name.clone(),
            None => derive_table_name(&class.path),
        };
        let Some(table) = schema.tables.get(&table_name) else { continue };
        for (col_name, col) in &table.columns {
            class.methods.entry(col_name.clone()).or_insert_with(|| MethodSig {
                required: 0,
                optional: 0,
                rest: false,
                keywords: Vec::new(),
                kwrest: false,
                sig: Some(RbsSig { params: Vec::new(), ret: col_rbs_ty(&col.type_name), type_params: Vec::new() }),
                sorbet_sig: None,
                sorbet_annotated: false,
                positional_names: None,
                nesting: Vec::new(),
                arity_unknown: false,
                abstract_stub: false,
                file: schema_file,
                def_span: col.span,
                name_span: col.span,
                schema_col_type: None,
            });
            class.methods.entry(format!("{col_name}=")).or_insert_with(|| MethodSig {
                required: 1,
                optional: 0,
                rest: false,
                keywords: Vec::new(),
                kwrest: false,
                // Empty `params` deliberately: a populated RBS param here
                // would run E0103's *strict* compatibility check
                // (`"42"` != `Integer`), which is exactly the false
                // positive invariant #1 forbids. The permissive E0106 check
                // in `check.rs` reads `schema_col_type` directly instead.
                sig: Some(RbsSig {
                    params: Vec::new(),
                    ret: crate::rbs_comment::RbsTy::Simple("void".to_string()),
                    type_params: Vec::new(),
                }),
                sorbet_sig: None,
                sorbet_annotated: false,
                positional_names: None,
                nesting: Vec::new(),
                arity_unknown: false,
                abstract_stub: false,
                file: schema_file,
                def_span: col.span,
                name_span: col.span,
                schema_col_type: Some(col.type_name.clone()),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_schema_has_no_tables() {
        let idx = parse_schema_text("");
        assert!(idx.tables.is_empty());
    }

    #[test]
    fn malformed_schema_degrades_to_empty_never_panics() {
        let idx = parse_schema_text("ActiveRecord::Schema.define do\n  create_table \"x\" do |t|\n");
        // Truncated/broken input: prism recovers or errors internally, this
        // must never panic and must never fabricate a table.
        assert!(idx.tables.is_empty() || idx.tables.contains_key("x"));
    }

    #[test]
    fn schema_without_create_table_is_empty() {
        let idx = parse_schema_text("enable_extension \"pgcrypto\"\ncreate_enum \"status\", [\"a\", \"b\"]\n");
        assert!(idx.tables.is_empty());
    }

    #[test]
    fn parses_columns_and_implicit_id() {
        let idx = parse_schema_text(
            r#"
            ActiveRecord::Schema[7.1].define(version: 1) do
              create_table "widgets", force: :cascade do |t|
                t.string "title"
                t.integer "quantity"
                t.index ["title"], name: "index_widgets_on_title"
              end
            end
            "#,
        );
        let table = idx.tables.get("widgets").expect("widgets table");
        assert_eq!(table.columns.len(), 3, "id + title + quantity, index ignored: {table:?}");
        assert_eq!(table.columns["id"].type_name, "integer");
        assert_eq!(table.columns["title"].type_name, "string");
        assert_eq!(table.columns["quantity"].type_name, "integer");
    }

    #[test]
    fn id_false_skips_implicit_id() {
        let idx = parse_schema_text(
            r#"
            create_table "widgets", id: false, force: :cascade do |t|
              t.string "title"
            end
            "#,
        );
        let table = idx.tables.get("widgets").expect("widgets table");
        assert!(!table.columns.contains_key("id"));
        assert_eq!(table.columns.len(), 1);
    }

    #[test]
    fn column_form_is_recognized() {
        let idx = parse_schema_text(
            r#"
            create_table "widgets" do |t|
              t.column "legacy_flag", :boolean
            end
            "#,
        );
        let table = idx.tables.get("widgets").expect("widgets table");
        assert_eq!(table.columns["legacy_flag"].type_name, "boolean");
    }

    #[test]
    fn multiple_columns_per_type_call() {
        let idx = parse_schema_text(
            r#"
            create_table "widgets" do |t|
              t.string "name", "email"
            end
            "#,
        );
        let table = idx.tables.get("widgets").expect("widgets table");
        assert!(table.columns.contains_key("name"));
        assert!(table.columns.contains_key("email"));
    }

    #[test]
    fn t_index_never_becomes_a_column() {
        let idx = parse_schema_text(
            r#"
            create_table "widgets" do |t|
              t.integer "age"
              t.index ["age"], name: "index_widgets_on_age"
              t.references "owner"
              t.timestamps
            end
            "#,
        );
        let table = idx.tables.get("widgets").expect("widgets table");
        // id + age only: index/references/timestamps must never shadow or
        // fabricate a column.
        assert_eq!(table.columns.len(), 2, "{table:?}");
        assert_eq!(table.columns["age"].type_name, "integer");
    }

    #[test]
    fn type_mapping_table() {
        // One row per bead contract line.
        assert_eq!(col_rbs_ty("string"), crate::rbs_comment::RbsTy::Simple("String".to_string()));
        assert_eq!(col_rbs_ty("text"), crate::rbs_comment::RbsTy::Simple("String".to_string()));
        assert_eq!(col_rbs_ty("integer"), crate::rbs_comment::RbsTy::Simple("Integer".to_string()));
        assert_eq!(col_rbs_ty("bigint"), crate::rbs_comment::RbsTy::Simple("Integer".to_string()));
        assert_eq!(col_rbs_ty("boolean"), crate::rbs_comment::RbsTy::Simple("bool".to_string()));
        assert_eq!(col_rbs_ty("decimal"), crate::rbs_comment::RbsTy::Simple("untyped".to_string()));
        assert_eq!(col_rbs_ty("date"), crate::rbs_comment::RbsTy::Simple("untyped".to_string()));
        assert_eq!(col_rbs_ty("datetime"), crate::rbs_comment::RbsTy::Simple("untyped".to_string()));
        assert_eq!(col_rbs_ty("json"), crate::rbs_comment::RbsTy::Simple("untyped".to_string()));
        assert_eq!(col_rbs_ty("jsonb"), crate::rbs_comment::RbsTy::Simple("untyped".to_string()));
        assert_eq!(col_rbs_ty("inet"), crate::rbs_comment::RbsTy::Simple("untyped".to_string()));

        assert_eq!(cast_risk("integer"), Some(CastRisk::Numeric));
        assert_eq!(cast_risk("bigint"), Some(CastRisk::Numeric));
        assert_eq!(cast_risk("decimal"), Some(CastRisk::Numeric));
        assert_eq!(cast_risk("date"), Some(CastRisk::Temporal));
        assert_eq!(cast_risk("datetime"), Some(CastRisk::Temporal));
        for other in ["string", "text", "boolean", "json", "jsonb", "inet"] {
            assert_eq!(cast_risk(other), None, "{other} must never risk E0106");
        }
    }

    #[test]
    fn derive_table_name_convention() {
        assert_eq!(derive_table_name("Widget"), "widgets");
        assert_eq!(derive_table_name("Category"), "categories");
        assert_eq!(derive_table_name("Box"), "boxes");
        assert_eq!(derive_table_name("Shop::BlogPost"), "blog_posts");
    }
}
