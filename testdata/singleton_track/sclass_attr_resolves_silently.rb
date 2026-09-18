# `class << self` + `attr_*` defines the accessors on the CLASS OBJECT.
# MRI runs this file to completion: every call below is real.
class Config
  class << self
    attr_accessor :endpoint
    attr_reader :region
    attr_writer :token
  end

  @region = "us-east-1"
end

Config.endpoint = "https://example.test"
raise "reader" unless Config.endpoint == "https://example.test"
raise "region" unless Config.region == "us-east-1"
Config.token = "secret"
