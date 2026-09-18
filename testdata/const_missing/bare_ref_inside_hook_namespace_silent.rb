# Ruby calls `const_missing` on the CREF, so a bare reference inside the
# very module that defines the hook resolves at runtime — this file exits 0
# under MRI. Warning here was a false positive (found by review 2026-09-17).
module Plugins
  def self.const_missing(name)
    const_set(name, Struct.new(:config))
  end

  def self.build
    Markdown.new({ smart: true })
  end
end

Plugins.build
