# Scope control: the hook belongs to a DIFFERENT namespace. `Plugins` has
# none of its own, so `Plugins::Markdown` is still unresolved and must still
# warn — a hook anywhere in the file never opens every namespace in it.
module Extensions
  def self.const_missing(name)
    const_set(name, Struct.new(:config))
  end
end

module Plugins
end

Plugins::Markdown
