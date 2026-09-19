# frozen_string_literal: true

# Bead ita-w2c, bead C control: `respond_to?` is a statement about the
# RECEIVER it is sent to — here `self` — and says nothing about the
# receiver of the guarded call. `self` really does answer
# `w2c_absent_elsewhere`, so the branch runs, and `Helper.new` really does
# not: MRI raises NoMethodError on the blamed line.
module W2cGuardNarrowingRespondToOtherReceiverAccuses
  class Helper; end

  class Converter
    def run
      if respond_to?(:w2c_absent_elsewhere)
        Helper.new.w2c_absent_elsewhere
      end
    end

    def w2c_absent_elsewhere
      :ok
    end
  end
end

W2cGuardNarrowingRespondToOtherReceiverAccuses::Converter.new.run