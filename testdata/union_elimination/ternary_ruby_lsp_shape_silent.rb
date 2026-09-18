# Bead ita-zdy: the ruby-lsp `declaration_listener.rb:512/515` shape —
# `name_or_nesting.is_a?(Array) ? name_or_nesting : Index.actual_nesting(@stack, name_or_nesting)`
# — a ternary whose false branch feeds a `String`-only param. Before this
# bead, `is_a?`'s false branch carried no information, so
# `name_or_nesting` stayed `String | Array[String]` there; passing a
# `Array[String]` half of that union into a `String`-only param is an
# E0103 under `compatible`'s "every union member must satisfy the param"
# rule — a real false positive Sorbet does not report, because Sorbet
# eliminates `Array` from the union in the false branch. Requires
# closed-world (bead ita-2ve): `is_a?(Array)` is a core-class check.
class UnionElimTernarySink
  #: (String) -> String
  def take_string(only_string)
    only_string
  end
end

class UnionElimTernaryWidget
  #: (String | Array[String]) -> String
  def normalize(name_or_nesting)
    sink = UnionElimTernarySink.new
    name_or_nesting.is_a?(Array) ? name_or_nesting.join : sink.take_string(name_or_nesting)
  end
end
