# discourse's GlobalSetting shape: every class method installed by
# `define_singleton_method(<non-literal>)` inside a `def self.*` body.
# Def bodies are never walked, so the class used to look CLOSED with none
# of these names in it - a guaranteed invariant #1 violation the moment
# the singleton NotFound arm reports. It must be OPEN.
class Settings
  def self.load(keys)
    keys.each do |key|
      define_singleton_method(key) { "value:#{key}" }
    end
  end

  def self.known(name)
    name
  end
end

Settings.load(%w[timeout retries])

raise "expected the dynamic class method" unless Settings.timeout == "value:timeout"
