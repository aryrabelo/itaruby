# frozen_string_literal: true

# Bead ita-scl, RESOLUTION side: an `include` inside `class << self` lands
# on the SINGLETON class's ancestry, so its instance methods are class
# methods of the receiver — mastodon's `class << self; include Redisable`
# (`app/lib/delivery_failure_tracker.rb:50`) is the measured shape, and
# the bare `redis` in `def warning_domains` read as a conclusive miss.
module SclRedisable
  def scl_redis
    :redis
  end
end

class SclTracker
  class << self
    include SclRedisable

    def scl_warning_domains
      scl_redis
    end
  end
end

raise "bad" unless SclTracker.scl_warning_domains == :redis
