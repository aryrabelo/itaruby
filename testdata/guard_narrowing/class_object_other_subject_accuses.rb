# frozen_string_literal: true

# Bead ita-w2c, bead C control: the proof attaches to the SUBJECT, never
# to the `&&`. Here the left operand proves `klass` is a class object,
# while the right operand's receiver is a different expression (`other`,
# a plain `Other` instance) — untouched by the guard, and genuinely
# missing `<`. MRI: the left operand is true, so the right one really
# runs and raises NoMethodError on the blamed line.
module W2cGuardNarrowingClassObjectOtherSubjectAccuses
  class Base; end

  class Other; end

  class Endpoint; end

  def self.klass
    Endpoint
  end

  def self.engine?
    other = Other.new
    klass.is_a?(Class) && other < Base
  end
end

W2cGuardNarrowingClassObjectOtherSubjectAccuses.engine?