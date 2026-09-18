# `extend self` copies the instance methods onto the module's singleton.
# MRI raises ArgumentError on the last line (`given 2, expected 1`).
module Util
  extend self

  def helper(a)
    a * 2
  end
end

Util.helper(1, 2)
