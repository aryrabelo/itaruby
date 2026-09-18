# Bead ita-54k G1: a reference nested through the qualified alias
# (`ConstAliasQualOwner::Bridge::CONST_ALIAS_QUAL_NESTED`) must resolve
# exactly as `ConstAliasQualTarget::CONST_ALIAS_QUAL_NESTED` would.
# Silent: no E0104.
class ConstAliasQualReader
  def read
    ConstAliasQualOwner::Bridge::CONST_ALIAS_QUAL_NESTED
  end
end
