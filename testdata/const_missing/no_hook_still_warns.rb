# The control: the SAME shape with no `const_missing` hook. Nothing
# autovivifies `Markdown`, the reference really is unresolved, and the
# warning must survive.
module Plugins
  def self.build(name)
    name
  end
end

Plugins::Markdown
