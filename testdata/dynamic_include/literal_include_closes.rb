module Greeter
  def hello
    "hi"
  end
end

class LiteralIncludeTarget
  include Greeter

  def use_it
    hello
    nonexistent_method
  end
end
