# `X.singleton_class.prepend M` puts M's INSTANCE methods on X's class
# object - discourse's spec/support/discourse_event_helper.rb does exactly
# this to give `DiscourseEvent.track_events` to 205 spec sites.
class Bus
  def self.trigger(name)
    "trigger:#{name}"
  end
end

module BusTestHelper
  def track(name)
    "track:#{trigger(name)}"
  end
end

Bus.singleton_class.prepend BusTestHelper

raise "expected the prepended class method" unless Bus.track("x") == "track:trigger:x"
