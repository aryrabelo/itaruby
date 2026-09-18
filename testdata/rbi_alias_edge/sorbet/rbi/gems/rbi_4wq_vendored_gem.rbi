# typed: true
# DO NOT EDIT MANUALLY — toy fixture for bead ita-4wq, mirroring the real
# tapioca/ruby-lsp shape: an RBI-internal alias (`Rbi4wqRubyLsp::Constant =
# Rbi4wqLanguageServer::Protocol::Constant`) re-exporting one gem's
# namespace under another, written directly in the vendored `.rbi` —
# never project code — with the real enum member declared as a qualified
# `T.let` write under the ALIAS'S TARGET namespace. Names are invented
# (globally unique `Rbi4wq*` prefix per AGENTS.md/bead ita-u1t), not the
# real gems.

module Rbi4wqRubyLsp
end

module Rbi4wqRubyLsp::Tapioca
end

module Rbi4wqLanguageServer
end

module Rbi4wqLanguageServer::Protocol
end

module Rbi4wqLanguageServer::Protocol::Constant
end

module Rbi4wqLanguageServer::Protocol::Constant::MessageType
end

Rbi4wqLanguageServer::Protocol::Constant::MessageType::WARNING = T.let(2, Integer)

Rbi4wqRubyLsp::Constant = Rbi4wqLanguageServer::Protocol::Constant
