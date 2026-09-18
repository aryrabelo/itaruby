# Same shape, all calls real: MRI runs this to completion.
module Util
  extend self

  def helper(a)
    a * 2
  end
end

raise "helper" unless Util.helper(2) == 4
