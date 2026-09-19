# frozen_string_literal: true

# Bead ita-w2c, bead C control: the same absent call with NO guard at
# all. MRI raises NoMethodError on the blamed line.
module W2cGuardNarrowingRespondToUnguardedAccuses
  class Converter
    def run
      w2c_absent_setup
    end
  end
end

W2cGuardNarrowingRespondToUnguardedAccuses::Converter.new.run