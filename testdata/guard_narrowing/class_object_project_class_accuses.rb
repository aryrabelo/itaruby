# frozen_string_literal: true

# Bead ita-w2c, bead C control: the guard keys on the core `Class`/
# `Module` names. `thing.is_a?(Base)` proves `thing` is a `Base` — a fact
# about an ORDINARY class, which the checker models perfectly well — so
# the right operand keeps its type and the missing `<` is still reported.
# MRI: the left operand is true, the right one runs, NoMethodError on the
# blamed line.
module W2cGuardNarrowingClassObjectProjectClassAccuses
  class Base; end

  def self.engine?
    thing = Base.new
    thing.is_a?(Base) && thing < Base
  end
end

W2cGuardNarrowingClassObjectProjectClassAccuses.engine?