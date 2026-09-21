# frozen_string_literal: true

# Bead ita-nst, INSTANCE side: a plain `def helper` inside a plain-yield
# block inside an INSTANCE method defines on the class of self — this very
# class — so the instance track files it. MRI runs to completion.
class Maker
  def make(&block)
    block.call
  end

  def build
    make { def helper; 41; end }
  end
end

Maker.new.build
raise "expected helper" unless Maker.new.helper == 41
