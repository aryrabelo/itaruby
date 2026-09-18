# frozen_string_literal: true

# An EXPLICIT-RECEIVER literal definer inside a `def` body says nothing
# about the enclosing class: `Other.define_method(:x)` defines `x` on
# Other, and `Host` must not collect it. Other's own surface becomes
# unknowable to the index (the definition happens whenever someone calls
# `Host.install`), so Other opens and its calls stay silent — never the
# other way round. MRI runs this file to completion.
class Other
end

class Host
  def self.install
    Other.define_method(:x) { 1 }
  end
end

Host.install
raise 'x' unless Other.new.x == 1
