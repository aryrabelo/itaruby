# Bead ita-y0s case c (mirrors ~/Sites/temp-files/prova-alias/c/main.rb):
# compact-form module header — already resolved before this bead
# (`rbi_map`'s exact key already carries the full qualified name).
# Sanity control: the new bare-last-segment fallback must never become
# the ONLY path that works — the pre-existing exact-match fast path
# stays byte-for-byte intact.
module RbiY0sCaseCApp
  Atalho = RbiY0sCaseCOrigem::Coisa
end

RbiY0sCaseCApp::Atalho::VALOR
