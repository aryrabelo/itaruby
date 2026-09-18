# The singleton reader takes no arguments: MRI raises ArgumentError on the
# last line (`wrong number of arguments (given 1, expected 0)`).
class Config
  class << self
    attr_accessor :endpoint
  end
end

Config.endpoint("https://example.test")
