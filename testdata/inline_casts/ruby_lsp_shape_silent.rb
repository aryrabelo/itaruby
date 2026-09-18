# Bead ita-qst: the exact ruby-lsp shape (lib/ruby_indexer/lib/ruby_indexer/
# index.rb:660) — a required `String` param fed a local `String | nil`
# value, cast non-nil by a trailing `#: as !nil` comment on the argument's
# own line. Without the cast, this fires E0103 ("expects String, got
# String | nil"); with it, silent.
class InlineCastNilSink
  #: (String) -> void
  def index_single(source)
  end
end

class InlineCastNilCaller
  #: (?String? source) -> void
  def handle_change(source = nil)
    InlineCastNilSink.new.index_single(
      source, #: as !nil
    )
  end
end
