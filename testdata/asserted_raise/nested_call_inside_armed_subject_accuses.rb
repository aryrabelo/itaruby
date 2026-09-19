# frozen_string_literal: true

# Bead ita-w2c, bead E control: the arm covers ONE call — the direct
# subject — never everything the subject's span happens to contain. Here
# the direct statement IS a call (`Guardian.new.tap { ... }`), but the
# diagnosed call sits inside that call's block, so it keeps firing. MRI
# proves the nested call really raises ArgumentError inside the asserted
# block (the matcher below catches it and the fixture asserts that).
module W2cAssertedRaiseNestedInSubjectAccuses
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

  def self.record(value)
    value
  end

  def self.run
    guardian = Guardian.new
    expect { record(guardian.can_edit_tag?) }.to raise_error(ArgumentError)
  end
end

raise 'the call inside the subject call must really raise' unless
  W2cAssertedRaiseNestedInSubjectAccuses.run == :raised_as_asserted