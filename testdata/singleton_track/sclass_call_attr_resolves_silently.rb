class Config
  singleton_class.attr_accessor :endpoint
  singleton_class.attr_reader :region
  singleton_class.attr_writer :token

  def self.connect
    endpoint
  end
end

Config.endpoint = "https://example.test"
Config.connect
Config.region
Config.token = "t2"
