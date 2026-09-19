# frozen_string_literal: true

# Bead ita-w2c, bead E, the minitest half: `assert_raises(...)` takes the
# raising code as its BLOCK, and the block body's direct statement is the
# subject. Same asserted exception, same silence — a different assertion
# spelling.
module W2cAssertedRaiseMinitestSilent
  class Guardian
    def can_edit_tag?(tag)
      tag
    end
  end

  def self.assert_raises(expected)
    yield
    raise "expected #{expected}"
  rescue expected
    :raised_as_asserted
  end

  def self.run
    assert_raises(ArgumentError) { Guardian.new.can_edit_tag? }
  end
end

raise 'silent fixture must catch the asserted ArgumentError' unless
  W2cAssertedRaiseMinitestSilent.run == :raised_as_asserted