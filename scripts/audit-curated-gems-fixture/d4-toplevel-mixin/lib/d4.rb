# Item 4: main forwards a bare top-level `include`/`prepend`/
# `define_method` to `Object`; `extend` lands on main's singleton.
module D4Mixin
  def d4_mixed = 1
end

include D4Mixin
prepend D4Mixin
define_method(:d4_defined) { 1 }
extend D4Mixin
