# Bead ita-3dg control: a qualified reference whose OWNER is not
# declared anywhere (not in the project, not in any vendored RBI) must
# still warn E0104. Guards against a mutant that treats
# `rbi_qualified_const_declares` (or `check_const_ref`'s new call to it)
# as a hit whenever ANY owner segment happens to resolve in the rbi_map,
# rather than the owner named in that RBI file's own
# `qualified_writes`/`consts`.
class Rbi3dgControlReader
  def bad
    Rbi3dgNeverDeclaredAnywhere::Sub::WHAT
  end
end
