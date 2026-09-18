# Bead ita-9he shadowing control: a top-level cbase write
# (`::ConstVisCbaseShared = ...`) must reach ONLY the `toplevel_consts`
# bucket, and a module's own bare `CONST = ...` write must never leak into
# `toplevel_consts` either -- `DefWalker`'s new cbase arm routes a write
# there regardless of `frag_idx` (cbase ignores lexical nesting, matching
# real Ruby), which must NOT be confused with widening the ORDINARY
# `ConstantWriteNode` arm's `frag_idx == None` check. If the write-side fix
# is ever broadened to feed `toplevel_consts` from ANY write regardless of
# `frag_idx` (not just a genuine cbase target), `ConstVisCbaseOutsider`'s
# bare reference to the module-scoped constant below would wrongly go
# silent instead of warning.
::ConstVisCbaseShared = "top-level connection"

module ConstVisCbaseOwnScope
  ConstVisCbaseModuleOnly = "module-scoped only"
end

class ConstVisCbaseOutsider
  def shared
    ConstVisCbaseShared
  end

  def leak_check
    ConstVisCbaseModuleOnly
  end
end
