# Bead ita-47y (RBI-target extension): a qualified-LHS alias inside a
# project namespace, whose target ALSO lives only in the vendorized gem
# RBI — the other measured ruby-lsp shape (`RubyLsp::Constant =
# LanguageServer::Protocol::Constant`, referenced from outside through
# the module's own qualifier).
module ConstAlias47yRbiNamespace
  Interface = ConstAlias47yVendoredGem::Interface
end
