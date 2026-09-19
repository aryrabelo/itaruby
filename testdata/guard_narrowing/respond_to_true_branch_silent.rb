# frozen_string_literal: true

# Bead ita-w2c, bead C side (i): `if respond_to?(:setup)` proves `self`
# really answers `setup` in the then-branch, which is the only place the
# call sits. Mirrors the discourse site
# `migrations/core/lib/migrations/conversion/base.rb:13-16`, where the
# base class has no `setup` and skips the call — an optional hook, not a
# missing method. (The `respond_to?(:m, true)` spelling has its own
# fixture: `respond_to_second_arg_silent.rb`.)
module W2cGuardNarrowingRespondToSilent
  class Converter
    def run
      if respond_to?(:setup)
        setup
      end
      :done
    end
  end
end

raise 'silent fixture must skip the guarded call' unless
  W2cGuardNarrowingRespondToSilent::Converter.new.run == :done