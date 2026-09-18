# typed: true
# const_missing autovivifies a namespace: `Plugins::Anything` resolves at
# runtime, and the method on it comes from the Struct it builds.
module Plugins
  def self.const_missing(name)
    const_set(name, Struct.new(:config))
  end
end

Plugins::Markdown.new({ smart: true }).config