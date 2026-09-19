# frozen_string_literal: true

# Bead ita-w2c, bead C control: the guard speaks about the CORE `Class`.
# This module declares its own `Class`, which shadows the core constant
# lexically, so `thing.is_a?(Class)` here asks about that ordinary class —
# a fact the checker models — and proves nothing about class objects.
# MRI: `thing` really is one of those, the left operand is true, and
# `thing < Base` raises NoMethodError on the blamed line.
module W2cGuardNarrowingClassObjectShadowAccuses
  class Base; end

  class Class; end

  def self.engine?
    thing = Class.new
    thing.is_a?(Class) && thing < Base
  end
end

W2cGuardNarrowingClassObjectShadowAccuses.engine?