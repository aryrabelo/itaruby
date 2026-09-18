module Minitest::Assertions
  def assert_equal(exp, act); end
end

class Minitest::Test < ::Minitest::Runnable
  include ::Minitest::Assertions
end
