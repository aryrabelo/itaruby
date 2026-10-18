# Mixin edges are surface too: this tree wrote these modules' ancestry. None
# of the three edges gives the module object `asist` (a typo of `assist`),
# so each call is a certain NoMethodError. Each module exercises one edge
# kind, so dropping any one of extend/include/prepend from the surface check
# silences exactly one of 33:9, 34:8 and 35:9.
module Toolsmith
  def assist
    :ok
  end
end

module Toolkit
  extend Toolsmith

  class Part
  end
end

module Kitbag
  include Toolsmith

  class Strap
  end
end

module Gearbox
  prepend Toolsmith

  class Cog
  end
end

Toolkit.asist
Kitbag.asist
Gearbox.asist
