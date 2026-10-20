# typed: true
# DO NOT EDIT MANUALLY — toy fixture for bead ita-h5s, mirroring
# planningalerts' real `sorbet/rbi/shims/elasticsearch_client.rbi`:
# `ElasticSearchClient = T.let(T.unsafe(nil), T.nilable(Elasticsearch
# ::Client))` — a BARE, unqualified top-level value-constant write, no
# `class`/`module` header anywhere in sight.
XClient = T.let(T.unsafe(nil), T.nilable(Integer))

# Phase-1/phase-2 split control: this heredoc BODY line starts at column
# 0 with `FakeConst = 1`, which `rbi.rs::scan_toplevel_const_write`'s
# cheap line scan cannot tell apart from a real assignment — it has no
# idea it is inside a string. Only `DESC` is a real top-level write; a
# real Prism parse (phase 2, `FileDefs::toplevel_consts`) never records
# `FakeConst` as one. If phase 2 verification were ever dropped (or
# replaced by "the candidate file exists"), `FakeConst` would wrongly
# resolve too.
DESC = <<~TEXT
FakeConst = 1
TEXT
