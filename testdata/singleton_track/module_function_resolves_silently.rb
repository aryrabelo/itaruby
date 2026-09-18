# `module_function` (bare, and the `module_function def x` form) puts the
# methods on the module's own singleton. MRI runs this file to completion.
module Bare
  module_function

  def helper(a)
    a * 2
  end
end

module Inline
  module_function def helper2(a)
    a + 1
  end
end

raise "bare" unless Bare.helper(2) == 4
raise "inline" unless Inline.helper2(1) == 2
