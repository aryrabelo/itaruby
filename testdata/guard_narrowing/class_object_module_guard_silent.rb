# frozen_string_literal: true

# Bead ita-w2c, bead C side (ii), the `Module` mirror: `is_a?(Module)`
# proves the same unknowable-as-a-`Ty` thing about a class object (every
# class IS a module). MRI: `rack_app` is the endpoint itself, the left
# operand is false, `&&` short-circuits to false — exit 0.
module W2cGuardNarrowingClassObjectModuleSilent
  class Base; end

  class Endpoint
    def app
      self
    end

    def rack_app
      app
    end

    def engine?
      rack_app.is_a?(Module) && rack_app < Base
    end
  end
end

raise 'silent fixture must short-circuit to false' unless
  W2cGuardNarrowingClassObjectModuleSilent::Endpoint.new.engine? == false