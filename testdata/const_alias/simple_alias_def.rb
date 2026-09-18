# Bead ita-54k G1: bare constant alias whose RHS is a literal constant
# path (`ConstantWriteNode` with a `ConstantReadNode` value).
# `ConstAliasSimpleTarget` is the real namespace; `ConstAliasSimpleBridge`
# is nothing but a name bound to it — the real ruby-lsp shape
# (`RubyLsp::Constant = LanguageServer::Protocol::Constant`, confirmed
# against the gem's own vendorized RBI in bead ita-40k).
module ConstAliasSimpleTarget
  CONST_ALIAS_SIMPLE_NESTED = 1
end

ConstAliasSimpleBridge = ConstAliasSimpleTarget
