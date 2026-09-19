# frozen_string_literal: true

# Bead ita-w2c, bead E control: the SAME zero-argument call to the
# one-argument `can_edit_tag?`, outside any assertion. MRI raises
# ArgumentError on the blamed line.
module W2cAssertedRaiseOutsideAccuses
  class Guardian
    def can_edit_tag?(tag)
      tag
    end
  end

  def self.run
    Guardian.new.can_edit_tag?
  end
end

W2cAssertedRaiseOutsideAccuses.run