# frozen_string_literal: true

# Bead ita-w2c, bead C control: the guard holds only in the branch that
# runs when the predicate was TRUE. The else-branch runs when
# `respond_to?` answered false — i.e. exactly when the method does NOT
# exist — so the same call there is a real missing method. MRI: this
# fixture raises NoMethodError on the blamed line.
module W2cGuardNarrowingRespondToElseAccuses
  class Converter
    def run
      if respond_to?(:w2c_absent_setup)
        :skipped
      else
        w2c_absent_setup
      end
    end
  end
end

W2cGuardNarrowingRespondToElseAccuses::Converter.new.run