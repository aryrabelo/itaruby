# frozen_string_literal: true

# Bead ita-scl, ACCUSATION side: the singleton-included method keeps its
# real signature, so the wrong-arity call on the class object is an E0102
# MRI really raises.
module SclArityRedisable
  def scl_keys(pattern)
    pattern
  end
end

class SclArityTracker
  class << self
    include SclArityRedisable
  end
end

SclArityTracker.scl_keys
