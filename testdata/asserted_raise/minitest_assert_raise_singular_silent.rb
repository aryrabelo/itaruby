# frozen_string_literal: true

# Bead ita-w2c, bead E: minitest spells the assertion BOTH ways —
# `assert_raise` and `assert_raises` — and the older singular form is the
# one this fixture pins, so the singular spelling cannot be dropped from
# the guard unnoticed.
module W2cAssertedRaiseMinitestAliasSilent
  class Guardian
    def can_edit_tag?(tag)
      tag
    end
  end

  def self.assert_raise(expected)
    yield
    raise "expected #{expected}"
  rescue expected
    :raised_as_asserted
  end

  def self.run
    assert_raise(ArgumentError) { Guardian.new.can_edit_tag? }
  end
end

raise 'silent fixture must catch the asserted ArgumentError' unless
  W2cAssertedRaiseMinitestAliasSilent.run == :raised_as_asserted