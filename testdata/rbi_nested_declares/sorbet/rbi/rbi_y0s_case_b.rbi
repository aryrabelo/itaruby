# typed: true
# DO NOT EDIT MANUALLY — toy fixture for bead ita-y0s case b (mirrors
# ~/Sites/temp-files/prova-alias/b/sorbet/rbi/origem.rbi): NESTED module
# blocks — phase 1's line scanner trims leading whitespace, so `Coisa`
# registers under its bare simple name only, never the fully qualified
# `RbiY0sCaseBOrigem::Coisa` phase 2's real prism parse computes.

module RbiY0sCaseBOrigem
  module Coisa
    VALOR = T.let(1, Integer)
  end
end
