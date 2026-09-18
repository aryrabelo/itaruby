# Bead ita-y0s case a (mirrors ~/Sites/temp-files/prova-alias/a): real
# nesting in PROJECT CODE (no RBI involved at all) — already resolves
# via `ProjectIndex` and must stay silent, unchanged by this bead.
# Sanity control: proves the alias-chase + nested-module machinery
# itself is correct with no RBI fallback in the picture.
module RbiY0sCaseAOrigem
  module Coisa
    VALOR = 1
  end
end

module RbiY0sCaseAApp
  Atalho = RbiY0sCaseAOrigem::Coisa
end

RbiY0sCaseAApp::Atalho::VALOR
