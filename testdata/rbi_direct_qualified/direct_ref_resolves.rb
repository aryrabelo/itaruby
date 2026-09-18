# Bead ita-3dg: a DIRECT qualified reference (no alias anywhere in the
# way) to a vendored RBI's qualified const-write must resolve silently.
# Before this bead, wave 6 only ever consulted
# `rbi_qualified_const_declares` via `alias_rbi_leaf_declares` (the
# alias-expansion branch reached from `expand_unresolved_alias_target`),
# which requires `path` to itself be an unresolved literal alias — a
# reference with no alias in the way at all (the real
# `RuboCop::Version::STRING` shape) still emitted a false E0104. `bad`
# names a member the RBI genuinely never declares under the SAME owner
# (anti-suppression control: the fix must not become a blanket
# suppressor for everything under `Rbi3dgVendoredGem::Version`, only the
# constants the RBI's own `qualified_writes`/`consts` actually name).
class Rbi3dgDirectReader
  def read
    Rbi3dgVendoredGem::Version::STRING
  end

  def bad
    Rbi3dgVendoredGem::Version::NOPE
  end
end
