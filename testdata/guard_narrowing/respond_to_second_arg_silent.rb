# frozen_string_literal: true

# Bead ita-w2c, bead C side (i), the two-argument spelling: the second
# argument (`true` also counts private methods) does not change what the
# predicate proves — `self` really answers `setup` in the then-branch.
module W2cGuardNarrowingRespondToSecondArgSilent
  class Converter
    def run
      if respond_to?(:setup, true)
        setup
      end
      :done
    end
  end
end

raise 'silent fixture must skip the guarded call' unless
  W2cGuardNarrowingRespondToSecondArgSilent::Converter.new.run == :done