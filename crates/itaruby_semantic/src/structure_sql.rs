//! `db/structure.sql` as an external type declaration for `ActiveRecord`
//! attributes (bead ita-muf), read only when a project has no
//! `db/schema.rb` — corpus-b is the reference corpus (a real Postgres
//! dump, no `schema.rb` at all). `crates/itaruby/src/main.rs::find_upward`
//! guarantees `schema.rb` always wins when both exist; this module never
//! sees a project that also has a `schema.rb`.
//!
//! Hand-written line scanner over `CREATE TABLE` blocks, stdlib only (no
//! SQL parser crate, per the bead's ponytail constraint) — deliberately not
//! a general SQL parser: scope is exactly table name + column name/type,
//! matching the bead contract. `CONSTRAINT`/`PRIMARY KEY`/`FOREIGN KEY`/
//! `UNIQUE`/`CHECK`/`EXCLUDE` lines, `CREATE SEQUENCE`/`ALTER TABLE`/other
//! statements, and any line this scanner doesn't recognize as
//! `<identifier> <type>`, are all silently skipped — never a phantom
//! column, the same shape as `schema.rs`'s `t.index` skip rule.
//!
//! Feeds the exact same `SchemaIndex`/`apply_schema_attributes`/
//! `cast_risk`/`col_rbs_ty` pipeline `schema.rs` built for `schema.rb`:
//! this module only produces a `SchemaIndex` from a different source text,
//! via `canonicalize_sql_type` translating Postgres type names into the
//! same vocabulary `schema.rb`'s own columns already use (`"integer"`,
//! `"string"`, `"boolean"`) so it needs zero new entries in
//! `col_rbs_ty`/`cast_risk`. Anything this bucket doesn't recognize
//! (`numeric`, `timestamp*`, `json`, `jsonb`, `inet`, `uuid`, enum types,
//! arrays, ...) passes through unchanged: `col_rbs_ty`/`cast_risk` already
//! default any unrecognized name to `untyped`/no-risk, so an unmapped SQL
//! type degrades to `Ty::Unknown` for free (invariant #1: false negative,
//! never a false positive — `numeric` columns are a known, accepted gap
//! per the bead contract, not a bug).
//!
//! Wired through `StructureSqlProject` (`lib.rs`), a salsa input entirely
//! separate from `ProjectFiles`: the dump's `SourceFile` is never pushed
//! into that vec, so nothing in this crate can ever run `file_defs`/
//! `check_file`/prism over it — declarations-only by construction.
//! `project_index` (`index.rs`) only reads it when no `db/schema.rb` was
//! found in `ProjectFiles` either.

use std::collections::HashMap;

use itaruby_syntax::SourceFile;

use crate::schema::{SchemaColumn, SchemaIndex, SchemaTable};

/// Parse `db/structure.sql`'s text. Depends only on this file's text, same
/// incrementality shape as `schema::schema_of_file`: editing a model never
/// reparses the (potentially large) dump.
#[salsa::tracked]
pub fn structure_sql_of_file(db: &dyn salsa::Database, file: SourceFile) -> SchemaIndex {
    parse_structure_sql_text(file.text(db))
}

/// Pure parse, independent of salsa — the unit-testable core. Degrades to
/// an empty index on empty input, truncated input (a `CREATE TABLE` opened
/// but never closed by a `);` line), or any syntax this scanner doesn't
/// recognize — never panics.
pub fn parse_structure_sql_text(text: &str) -> SchemaIndex {
    let mut tables = HashMap::new();
    let mut pos = 0usize;
    while pos < text.len() {
        let (header, _start, next_pos) = next_line(text, pos);
        pos = next_pos;
        let trimmed = header.trim_start();
        let is_create_table =
            trimmed.get(..12).is_some_and(|h| h.eq_ignore_ascii_case("CREATE TABLE"));
        if !is_create_table {
            continue;
        }
        let Some(name) = parse_table_name(trimmed) else { continue };

        let mut table = SchemaTable::default();
        let mut closed = false;
        while pos < text.len() {
            let (body_line, body_start, body_next) = next_line(text, pos);
            pos = body_next;
            let body_trimmed = body_line.trim();
            if body_trimmed.starts_with(");") {
                closed = true;
                break;
            }
            if let Some((col_name, type_name)) = parse_column_line(body_line) {
                table.columns.insert(
                    col_name,
                    SchemaColumn {
                        type_name: canonicalize_sql_type(&type_name),
                        span: (body_start, body_start + body_line.len()),
                    },
                );
            }
        }
        // Truncated input (EOF before a `);` line) discards the whole
        // table rather than keeping a partial column set — the bead's
        // degradation contract wants "zero tables", not "best effort".
        if closed {
            tables.insert(name, table);
        }
    }
    SchemaIndex { tables }
}

/// One line, without its trailing `\n`/`\r\n`: `(text, start offset, offset
/// of the following line)`. Never panics — every slice point sits on a
/// `\n` byte, always a valid `char` boundary.
fn next_line(text: &str, pos: usize) -> (&str, usize, usize) {
    let rest = &text[pos..];
    match rest.find('\n') {
        Some(i) => (rest[..i].trim_end_matches('\r'), pos, pos + i + 1),
        None => (rest.trim_end_matches('\r'), pos, text.len()),
    }
}

/// `CREATE TABLE public.widgets (` / `CREATE TABLE widgets (` / a quoted
/// identifier in either position. Returns the bare table name — the schema
/// prefix (`public.`), if any, is dropped: `apply_schema_attributes` looks
/// tables up by bare name, matching `derive_table_name`'s own convention.
fn parse_table_name(header: &str) -> Option<String> {
    let rest = header.get(12..)?.trim_start();
    let paren = rest.find('(')?;
    let ident_part = rest[..paren].trim();
    if ident_part.is_empty() {
        return None;
    }
    let after_schema = match ident_part.rfind('.') {
        Some(i) => &ident_part[i + 1..],
        None => ident_part,
    };
    let name = strip_quotes(after_schema.trim());
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

fn strip_quotes(s: &str) -> String {
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

/// Keywords that open a table-level constraint or clause, never a column
/// definition — mirrors `schema.rs`'s `t.index`/`t.references` skip: an
/// unrecognized line is not a phantom column, ever.
const NON_COLUMN_PREFIXES: &[&str] =
    &["CONSTRAINT", "PRIMARY KEY", "FOREIGN KEY", "UNIQUE", "CHECK", "EXCLUDE", "LIKE"];

/// One `<column> <type> [modifiers...]` line inside a `CREATE TABLE` body.
/// Returns `None` for constraint lines, blank lines, and anything that
/// doesn't parse as an identifier followed by at least one type token.
fn parse_column_line(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim();
    let trimmed = trimmed.strip_suffix(',').unwrap_or(trimmed).trim();
    if trimmed.is_empty() {
        return None;
    }
    let upper = trimmed.to_ascii_uppercase();
    if NON_COLUMN_PREFIXES.iter().any(|kw| upper.starts_with(kw)) {
        return None;
    }
    let (ident, rest) = split_identifier(trimmed)?;
    let type_name = extract_type(rest.trim())?;
    Some((ident, type_name.to_ascii_lowercase()))
}

/// `"quoted identifier"` or a bare `[A-Za-z_][A-Za-z0-9_]*` word, followed
/// by whitespace and the rest of the line.
fn split_identifier(s: &str) -> Option<(String, &str)> {
    if let Some(rest) = s.strip_prefix('"') {
        let end = rest.find('"')?;
        return Some((rest[..end].to_string(), &rest[end + 1..]));
    }
    let end = s.find(char::is_whitespace)?;
    let ident = &s[..end];
    let starts_ok = ident.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_');
    if ident.is_empty() || !starts_ok {
        return None;
    }
    Some((ident.to_string(), &s[end..]))
}

/// Stop words that end a type: everything before the first one is the raw
/// type name. `WITHOUT`/`WITH`/`TIME`/`ZONE` are deliberately not stop
/// words, so `timestamp without time zone` stays one type name — it still
/// degrades to `Ty::Unknown` downstream either way, so this only matters
/// for readability of the stored (unused for diagnostics) type name.
const TYPE_STOP_WORDS: &[&str] = &[
    "NOT", "NULL", "DEFAULT", "REFERENCES", "GENERATED", "COLLATE", "CHECK", "UNIQUE", "PRIMARY",
    "CONSTRAINT",
];

fn extract_type(rest: &str) -> Option<String> {
    let mut tokens = Vec::new();
    for tok in rest.split_whitespace() {
        if TYPE_STOP_WORDS.contains(&tok.to_ascii_uppercase().as_str()) {
            break;
        }
        tokens.push(tok);
    }
    if tokens.is_empty() {
        return None;
    }
    Some(strip_precision(&tokens.join(" ")))
}

/// Drops a trailing `(255)`/`(10,2)` precision/length suffix.
fn strip_precision(s: &str) -> String {
    if s.ends_with(')') {
        if let Some(open) = s.rfind('(') {
            return s[..open].trim_end().to_string();
        }
    }
    s.to_string()
}

/// Translates a raw SQL type name into the vocabulary `schema.rs`'s
/// `col_rbs_ty`/`cast_risk` already understand. Anything not listed here is
/// returned unchanged — those two functions already default an
/// unrecognized name to `untyped`/no-risk, so this bucket only needs to
/// name the types it actually wants to promote out of `Ty::Unknown`.
fn canonicalize_sql_type(raw: &str) -> String {
    match raw {
        "smallint" | "integer" | "bigint" | "serial" | "bigserial" => "integer".to_string(),
        "text" | "character varying" | "varchar" | "character" | "char" | "citext" => {
            "string".to_string()
        }
        "boolean" => "boolean".to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_text_has_no_tables() {
        assert!(parse_structure_sql_text("").tables.is_empty());
    }

    #[test]
    fn truncated_create_table_degrades_to_no_tables() {
        // Opened but never closed by a `);` line.
        let idx = parse_structure_sql_text(
            "CREATE TABLE public.widgets (\n    id bigint NOT NULL,\n    name text\n",
        );
        assert!(idx.tables.is_empty(), "{idx:?}");
    }

    #[test]
    fn unrecognized_syntax_degrades_to_no_tables() {
        let idx = parse_structure_sql_text(
            "-- pg_dump preamble\nSET statement_timeout = 0;\nSELECT pg_catalog.setval('x', 1);\n",
        );
        assert!(idx.tables.is_empty(), "{idx:?}");
    }

    #[test]
    fn parses_create_table_with_schema_prefix() {
        let idx = parse_structure_sql_text(
            "CREATE TABLE public.widgets (\n    id bigint NOT NULL,\n    name text\n);\n",
        );
        let table = idx.tables.get("widgets").expect("widgets table");
        assert_eq!(table.columns.len(), 2, "{table:?}");
        assert_eq!(table.columns["id"].type_name, "integer");
        assert_eq!(table.columns["name"].type_name, "string");
    }

    #[test]
    fn parses_bare_create_table() {
        let idx = parse_structure_sql_text(
            "CREATE TABLE sprockets (\n    id bigint NOT NULL,\n    count integer\n);\n",
        );
        assert!(idx.tables.contains_key("sprockets"));
    }

    #[test]
    fn quoted_identifier_precision_suffix_and_constraint_lines_ignored() {
        let idx = parse_structure_sql_text(
            "CREATE TABLE public.widgets (\n\
             \x20   id bigint NOT NULL,\n\
             \x20   \"label\" character varying(255),\n\
             \x20   quantity integer,\n\
             \x20   price numeric(10,2),\n\
             \x20   active boolean DEFAULT true,\n\
             \x20   CONSTRAINT widgets_price_check CHECK ((price >= (0)::numeric)),\n\
             \x20   PRIMARY KEY (id)\n\
             );\n",
        );
        let table = idx.tables.get("widgets").expect("widgets table");
        assert_eq!(
            table.columns.len(),
            5,
            "constraint/primary key must never become columns: {table:?}"
        );
        assert_eq!(table.columns["label"].type_name, "string");
        assert_eq!(table.columns["quantity"].type_name, "integer");
        assert_eq!(
            table.columns["price"].type_name, "numeric",
            "unmapped -> passthrough, not a bucket alias"
        );
        assert_eq!(table.columns["active"].type_name, "boolean");
    }

    #[test]
    fn type_bucket_matches_schema_rb_vocabulary() {
        use crate::rbs_comment::RbsTy;
        use crate::schema::{cast_risk, col_rbs_ty, CastRisk};

        for raw in ["integer", "smallint", "bigint", "serial", "bigserial"] {
            assert_eq!(canonicalize_sql_type(raw), "integer", "{raw}");
        }
        for raw in ["text", "character varying", "varchar", "character", "char", "citext"] {
            assert_eq!(canonicalize_sql_type(raw), "string", "{raw}");
        }
        assert_eq!(canonicalize_sql_type("boolean"), "boolean");

        // Unmapped -> passthrough -> schema.rs's own untyped read default
        // takes over (never a real `Ty`), no second unknown-type marker
        // needed. Reads are always Unknown; write-side cast risk is
        // whatever schema.rs's own `cast_risk` already says for that raw
        // name — "date"/"datetime" inherit `CastRisk::Temporal` because
        // schema.rb's own dates already get it (same map, same audited
        // rule), everything else here has no entry in `cast_risk` at all.
        for raw in [
            "numeric",
            "timestamp",
            "timestamp without time zone",
            "double precision",
            "json",
            "jsonb",
            "inet",
            "uuid",
        ] {
            let mapped = canonicalize_sql_type(raw);
            assert_eq!(mapped, raw);
            assert_eq!(
                col_rbs_ty(&mapped),
                RbsTy::Simple("untyped".to_string()),
                "{raw}"
            );
            assert_eq!(cast_risk(&mapped), None, "{raw}");
        }
        for raw in ["date", "datetime"] {
            let mapped = canonicalize_sql_type(raw);
            assert_eq!(mapped, raw);
            assert_eq!(
                col_rbs_ty(&mapped),
                RbsTy::Simple("untyped".to_string()),
                "{raw}"
            );
            assert_eq!(cast_risk(&mapped), Some(CastRisk::Temporal), "{raw}");
        }

        assert_eq!(cast_risk("integer"), Some(CastRisk::Numeric));
        assert_eq!(cast_risk("boolean"), None);
        assert_eq!(cast_risk("string"), None);
    }

    #[test]
    fn multiple_tables_in_one_dump_and_column_count() {
        let idx = parse_structure_sql_text(
            "CREATE TABLE public.widgets (\n\
             \x20   id bigint NOT NULL,\n\
             \x20   \"label\" character varying(255),\n\
             \x20   quantity integer,\n\
             \x20   price numeric(10,2),\n\
             \x20   active boolean DEFAULT true,\n\
             \x20   CONSTRAINT widgets_price_check CHECK ((price >= (0)::numeric)),\n\
             \x20   PRIMARY KEY (id)\n\
             );\n\
             \n\
             CREATE TABLE sprockets (\n\
             \x20   id bigint NOT NULL,\n\
             \x20   count integer\n\
             );\n",
        );
        assert_eq!(idx.tables.len(), 2);
        let total: usize = idx.tables.values().map(|t| t.columns.len()).sum();
        assert_eq!(total, 7, "5 (widgets) + 2 (sprockets): {idx:?}");
    }

    #[test]
    fn non_create_table_statements_never_produce_phantom_tables() {
        let idx = parse_structure_sql_text(
            "CREATE SEQUENCE public.widgets_id_seq\n    START WITH 1\n    INCREMENT BY 1;\n\n\
             ALTER TABLE ONLY public.widgets ALTER COLUMN id SET DEFAULT nextval('x');\n",
        );
        assert!(idx.tables.is_empty(), "{idx:?}");
    }
}
