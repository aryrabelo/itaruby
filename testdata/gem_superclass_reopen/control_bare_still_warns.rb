# Control (bead ita-c8h gate G2): a brand-new top-level project class —
# no `::` anywhere in its path — must keep accusing exactly as before.
# `apply_undeclared_namespace_reopenings` only ever widens silence for a
# NESTED path (one with a `::`); a bare name is always the project's own
# to define, and treating it as an external reopening would kill a
# legitimate E0101 the checker exists to catch.
class GemSuperReopenControlBare
  def real_method
    1
  end
end

GemSuperReopenControlBare.new.nonexistent_method
