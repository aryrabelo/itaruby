# Bead ita-9he: a top-level constant WRITE in cbase form (`::X = v`) is a
# `ConstantPathWriteNode` whose target is a single-segment cbase path
# (`cpw.target().parent()` is `None` -- see `const_path_str`'s cbase
# branch). `DefWalker`'s `ConstantPathWriteNode` arm only ever fed
# `qualified_writes` when the trimmed full path still contained a `::`
# (the `A::B = v` shape) -- a bare `::X = v` trims to a single segment with
# no `::` left, so `rsplit_once("::")` returned `None` and the write was
# silently dropped: never reached `toplevel_consts`, `fragments[i].consts`,
# nor anything `const_exists` reads. Real Ruby defines this on `Object`
# exactly like a bare toplevel `X = v`. Silent; paired with
# cbase_write_bare_ref.rb.
::ConstVisCbaseDb = "connection"
