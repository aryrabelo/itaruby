# typed: true
# DO NOT EDIT MANUALLY — toy fixture for bead ita-47y's RBI-target
# extension, mirroring the shape Tapioca actually emits for a vendorized
# gem: one fully-qualified `class`/`module` header per declaration (never
# nested), plus qualified `T.let` constant writes with the FULL path on
# the LHS — the real ruby-lsp shape
# (`sorbet/rbi/gems/language_server-protocol@*.rbi` declares
# `module LanguageServer::Protocol::Constant::CompletionItemKind` this
# way, with its enum members written as qualified `T.let` writes, never
# nested inside the module body). Names are invented, not a real gem.

module ConstAlias47yVendoredGem::Interface::CompletionItemKind
end

ConstAlias47yVendoredGem::Interface::CompletionItemKind::FIELD = T.let(10, Integer)
