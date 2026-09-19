# frozen_string_literal: true

# Bead ita-w2c, bead C side (ii), the LOCAL-variable spelling of the same
# proof: `endpoint` is a plain local typed by its own assignment, and the
# left operand's `is_a?(Class)` is what makes the right operand's read of
# it unknowable-as-Endpoint. MRI short-circuits to false.
module W2cGuardNarrowingClassObjectLocalSilent
  class Base; end

  class Endpoint; end

  def self.engine?
    endpoint = Endpoint.new
    endpoint.is_a?(Class) && endpoint < Base
  end
end

raise 'silent fixture must short-circuit to false' unless
  W2cGuardNarrowingClassObjectLocalSilent.engine? == false