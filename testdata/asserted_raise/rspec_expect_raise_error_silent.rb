# frozen_string_literal: true

# Bead ita-w2c, bead E: the RSpec shape from
# `spec/lib/guardian/tag_guardian_spec.rb:98-100` — the call is the DIRECT
# subject of `expect { ... }.to raise_error(...)`, and the exception it
# raises (its arity, here) is the ASSERTED behavior, not a defect.
# `Guardian#can_edit_tag?` really does require its tag
# (`lib/guardian/tag_guardian.rb:18`).
#
# The doubles are the smallest honest stand-ins for the RSpec pair: `to`
# runs the block, the matcher proves the exception type. MRI: exits 0.
module W2cAssertedRaiseExpectSilent
  class Guardian
    def can_edit_tag?(tag)
      tag
    end
  end

  class Expectation
    def initialize(&blk)
      @blk = blk
    end

    def to(matcher)
      matcher.call(@blk)
    end
  end

  def self.expect(&blk)
    Expectation.new(&blk)
  end

  def self.raise_error(expected)
    lambda do |blk|
      blk.call
      raise "expected #{expected}"
    rescue expected
      :raised_as_asserted
    end
  end

  def self.run
    expect { Guardian.new.can_edit_tag? }.to raise_error(ArgumentError)
  end
end

raise 'silent fixture must catch the asserted ArgumentError' unless
  W2cAssertedRaiseExpectSilent.run == :raised_as_asserted