# typed: true
# Methods defined by iterating a literal list: `host`, `port`, `scheme` and
# their writers exist only after the class body runs.
class Settings
  [:host, :port, :scheme].each do |key|
    define_method(key) { @data[key] }
    define_method("#{key}=") { |value| @data[key] = value }
  end

  def initialize(data)
    @data = data
  end
end

config = Settings.new({ host: "localhost" })
config.host
config.port = 8080