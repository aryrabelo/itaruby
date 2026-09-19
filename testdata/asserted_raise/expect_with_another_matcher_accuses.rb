# frozen_string_literal: true

# Bead ita-w2c, bead E control: an `expect { ... }.to <matcher>` whose
# matcher is NOT `raise_error` asserts nothing about exceptions, so the
# subject keeps its diagnostics. MRI: the block really raises
# ArgumentError, and this matcher (unlike a raise matcher) does not catch
# it — exit 1 on the blamed line.
module W2cAssertedRaiseOtherMatcherAccuses
  class Guardian
    def can_edit_tag?(tag)
      tag
    end
  end

  class Expectation
    def initialize(&blk)
      @blk = blk
    end

    def to(_matcher)
      @blk.call
    end
  end

  def self.expect(&blk)
    Expectation.new(&blk)
  end

  def self.eq(value)
    value
  end

  def self.run
    expect { Guardian.new.can_edit_tag? }.to eq(1)
  end
end

W2cAssertedRaiseOtherMatcherAccuses.run