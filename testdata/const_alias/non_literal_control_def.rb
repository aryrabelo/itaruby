# Bead ita-47y control: a non-literal RHS is NEVER treated as an alias —
# `const_aliases` only ever records a RHS that parsed as a bare/qualified
# constant path (`const_path_str`), and this bead does not widen that.
# Behavior here must be byte-identical to before this bead: neither write
# below registers any alias edge at all.
class ConstAlias47ySome
  def self.call
    1
  end
end

ConstAlias47yDynamicCall = ConstAlias47ySome.call
ConstAlias47yLiteralNumber = 42
