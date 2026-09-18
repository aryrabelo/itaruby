# typed: true
# A capability check guarding the call: whatever `driver` turns out to be,
# `reset!` is only reached when the receiver answers it. Both drivers below
# go through the guard and the program runs clean.
class Session
  def initialize(driver)
    @driver = driver
  end

  def reset
    @driver.reset! if @driver.respond_to?(:reset!)
  end
end

class ResettableDriver
  def reset!
    :reset
  end
end

class InertDriver
end

Session.new(ResettableDriver.new).reset
Session.new(InertDriver.new).reset