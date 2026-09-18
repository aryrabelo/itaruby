# Bead ita-47y (RBI-target extension): the alias's OWN target lives only
# in the toy sorbet/rbi/gems fixture next to this file — never as a
# project ClassId — exactly ruby-lsp's own real
# `Interface = LanguageServer::Protocol::Interface` (measured 90% owner
# of ruby-lsp's E0104 noise: the alias's own target is a vendorized gem
# constant, not project code).
ConstAlias47yRbiBridge = ConstAlias47yVendoredGem::Interface
