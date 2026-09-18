# Bead ita-54k G1: a reference NESTED through the alias
# (`ConstAliasSimpleBridge::CONST_ALIAS_SIMPLE_NESTED`) must resolve
# exactly as `ConstAliasSimpleTarget::CONST_ALIAS_SIMPLE_NESTED` would.
# Before this bead, `ConstAliasSimpleBridge` was never a real class path
# (no `class`/`module` keyword declared it — it's a plain assignment), so
# `resolve_const` could never resolve it as a prefix and the reference
# always warned a false E0104, even though a bare reference to
# `ConstAliasSimpleBridge` alone (no `::` suffix) already resolved
# trivially. Silent: no E0104.
class ConstAliasSimpleReader
  def read
    ConstAliasSimpleBridge::CONST_ALIAS_SIMPLE_NESTED
  end
end
