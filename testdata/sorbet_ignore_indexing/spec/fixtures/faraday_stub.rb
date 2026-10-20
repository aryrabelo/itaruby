# A test-fixture stub sharing a gem's name (no RBI declares the gem in
# this tree). Closed, nearly empty: if it were indexed like project code,
# `Faraday.get` would read as a conclusive miss on a method the real gem
# has.
class Faraday
  def self.stub_only
  end
end
