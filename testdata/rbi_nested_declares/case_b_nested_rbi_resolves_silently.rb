# Bead ita-y0s case b (mirrors ~/Sites/temp-files/prova-alias/b/main.rb):
# `RbiY0sCaseBOrigem` is never project code — it exists only in this
# fixture's own vendored `sorbet/rbi/rbi_y0s_case_b.rbi`, nested. Before
# this bead, `rbi_map.get("RbiY0sCaseBOrigem::Coisa")` missed outright
# (phase 1 only ever registered the bare `"Coisa"` key for a nested
# header), so BOTH the direct reference on the alias's own RHS
# (`RbiY0sCaseBOrigem::Coisa`) and the member reached through the alias
# (`Atalho::VALOR`) fired a false E0104. Both must now be silent.
module RbiY0sCaseBApp
  Atalho = RbiY0sCaseBOrigem::Coisa
end

RbiY0sCaseBApp::Atalho::VALOR
