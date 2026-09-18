class Config
  singleton_class.attr_accessor :endpoint
end

Config.endpoint = "https://example.test"
Config.endpoint
Config.endpoint(1)
