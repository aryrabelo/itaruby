# Bead ita-47y: measured ruby-lsp shape (lib/ruby_lsp/utils.rb:6-7) —
# `Interface = LanguageServer::Protocol::Interface` is a bare alias to a
# real namespace, referenced TWO segments past the alias
# (`Interface::CompletionItemKind::FIELD`), not the one-segment nesting
# bead ita-54k already covered (`Bridge::NESTED`). Separately, ruby-lsp's
# own `RubyLsp::Constant = LanguageServer::Protocol::Constant` aliases
# INSIDE a project module, referenced through that module's own
# qualifier from a different file
# (`RubyLsp::Constant::CompletionItemKind::FIELD`) — the other measured
# shape (36 more E0104s of the same kind, bead ita-47y's task brief).
module ConstAlias47yProtocol
  module Interface
    module CompletionItemKind
      FIELD = 10
    end
  end

  module Constant
    module CompletionItemKind
      FIELD = 3
    end
  end
end

# Bare alias — ruby-lsp's own `lib/ruby_lsp/utils.rb:6`.
ConstAlias47yInterface = ConstAlias47yProtocol::Interface

module ConstAlias47yRubyLsp
  # Qualified-LHS alias inside a project namespace — ruby-lsp's own
  # `RubyLsp::Constant = LanguageServer::Protocol::Constant`.
  Constant = ConstAlias47yProtocol::Constant
end
