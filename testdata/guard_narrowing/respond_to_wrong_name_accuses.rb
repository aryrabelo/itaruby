# frozen_string_literal: true

# Bead ita-w2c, bead C control: the guard is keyed on the EXACT name it
# names. `respond_to?(:w2c_present_step)` proves nothing about
# `w2c_absent_step`, whose call here is genuinely missing — MRI raises
# NoMethodError on the blamed line.
module W2cGuardNarrowingRespondToWrongNameAccuses
  class Converter
    def run
      if respond_to?(:w2c_present_step)
        w2c_absent_step
      end
    end

    def w2c_present_step
      :ok
    end
  end
end

W2cGuardNarrowingRespondToWrongNameAccuses::Converter.new.run