# frozen_string_literal: true

# Bead ita-w2c, bead C side (ii): `rack_app.is_a?(Class) && rack_app < Base`
# — the rails `endpoint.rb:14-16` shape. The left operand proves `rack_app`
# is a class OBJECT, so the second read of `rack_app` is not the enclosing
# Endpoint instance any more: the `<` dispatches on `Class`, whose method
# table no `Ty` models.
#
# MRI: `rack_app` returns the endpoint itself, `is_a?(Class)` is false, and
# `&&` short-circuits — the right operand can never run here.
module W2cGuardNarrowingClassObjectCallSilent
  class Base; end

  class Endpoint
    def app
      self
    end

    def rack_app
      app
    end

    def engine?
      rack_app.is_a?(Class) && rack_app < Base
    end
  end
end

raise 'silent fixture must short-circuit to false' unless
  W2cGuardNarrowingClassObjectCallSilent::Endpoint.new.engine? == false