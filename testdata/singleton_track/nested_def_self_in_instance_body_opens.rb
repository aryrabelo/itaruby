# frozen_string_literal: true

# Bead ita-nst, FAIL-CLOSED side: `def self.bolt` inside an INSTANCE
# method's block defines on ONE object's own singleton at runtime — an
# owner no index position can name. The class must OPEN (never collect a
# name it may not have). MRI runs to completion; `bolt` never lands on the
# class.
class Maker
  def make(&block)
    block.call
  end

  def build
    make { def self.bolt; 1; end }
    2
  end
end

Maker.new.build
