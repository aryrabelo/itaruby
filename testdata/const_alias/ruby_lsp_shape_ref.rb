# Bead ita-47y: both reference shapes measured in ruby-lsp — a bare alias
# walked two segments deep, and the qualified `RubyLsp::` form walked
# from OUTSIDE the aliasing module. Silent: no E0104 either way.
class ConstAlias47yReader
  def bare
    ConstAlias47yInterface::CompletionItemKind::FIELD
  end

  def qualified_namespace
    ConstAlias47yRubyLsp::Constant::CompletionItemKind::FIELD
  end
end
