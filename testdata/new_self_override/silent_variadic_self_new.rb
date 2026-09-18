# Bead ita-6bq: a class that defines its OWN singleton `new` with a
# variadic/forwarding signature (the real-world shape is
# `rails/activesupport`'s `ActiveSupport::Deprecation::DeprecationProxy`:
# `def self.new(*args, **kwargs, &block)`) makes the real accepted call
# shape unknowable from the signature alone. `initialize` below is a
# mismatched 0-arg decoy: if arity were still read from it, the 3-arg
# call would wrongly fire E0102 — the correct behavior is silence,
# governed by `self.new`, never a fabricated arity check (invariant #1).
class NewSelfOvVariadic
  def self.new(*args, **kwargs, &block)
    allocate
  end

  def initialize
  end
end

NewSelfOvVariadic.new(1, 2, 3)
