# Bead ita-54k G1: qualified-LHS alias (`A::B = C::D`, a
# `ConstantPathWriteNode`), RHS itself a literal constant path.
module ConstAliasQualOwner
end

module ConstAliasQualTarget
  CONST_ALIAS_QUAL_NESTED = 2
end

ConstAliasQualOwner::Bridge = ConstAliasQualTarget
