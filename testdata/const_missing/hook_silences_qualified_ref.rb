# A namespace that defines `self.const_missing` autovivifies constants at
# runtime: this program runs clean under MRI. Its surface is unknowable by
# construction, so an "unresolved constant" here would be a false positive.
module Plugins
  def self.const_missing(name)
    const_set(name, Struct.new(:config))
  end
end

Plugins::Markdown.new({ smart: true }).config
