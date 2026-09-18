# Defect B: a plain class method reading the top-level `CONSTVIS_LIST`
# (defined in toplevel_const_def.rb, the same project). Constant
# *definition* is lexical, and a top-level constant is visible everywhere
# through the bare-name widening search's top-level scope — but
# `const_exists`'s widening loop guards every iteration with
# `if !walked.is_empty()`, so the top-level scope is structurally never
# consulted. Must become silent.
class ConstVisReader
  def read
    CONSTVIS_LIST
  end
end
