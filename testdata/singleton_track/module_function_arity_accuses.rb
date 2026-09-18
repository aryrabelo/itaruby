# The module function takes exactly one argument: MRI raises ArgumentError
# on the last line (`given 2, expected 1`).
module Bare
  module_function

  def helper(a)
    a * 2
  end
end

Bare.helper(1, 2)
