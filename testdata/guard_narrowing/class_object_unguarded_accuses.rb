# frozen_string_literal: true

# Bead ita-w2c, bead C control: the same `rack_app < Base` with NO
# `is_a?(Class)` guard in front of it. Nothing proves `rack_app` is a
# class object, so the `<` really is missing on Endpoint — MRI raises
# NoMethodError on the blamed line.
module W2cGuardNarrowingClassObjectUnguardedAccuses
  class Base; end

  class Endpoint
    def app
      self
    end

    def rack_app
      app
    end

    def engine?
      rack_app < Base
    end
  end
end

W2cGuardNarrowingClassObjectUnguardedAccuses::Endpoint.new.engine?