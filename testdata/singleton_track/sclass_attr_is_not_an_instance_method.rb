# The other half of the same fact: the accessor is NOT on instances.
# MRI raises NoMethodError on the last line.
class Config
  class << self
    attr_accessor :endpoint
  end
end

Config.new.endpoint
