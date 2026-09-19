# frozen_string_literal: true

# Bead ita-w2c, bead E control, and the owner's deliberately NARROW scope:
# only the call that IS the direct statement of the asserted block is
# silenced. Here the subject sits inside an array literal, one level
# deeper, so it keeps firing — and at runtime it really does raise
# ArgumentError inside the block (the matcher below is what proves it).
module W2cAssertedRaiseNestedAccuses
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
    expect { [Guardian.new.can_edit_tag?] }.to raise_error(ArgumentError)
  end
end

raise 'the nested call must really raise inside the block' unless
  W2cAssertedRaiseNestedAccuses.run == :raised_as_asserted