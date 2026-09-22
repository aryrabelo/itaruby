# frozen_string_literal: true

# Bead ita-sgl, RESOLUTION side: `include Singleton` runs
# `Singleton.included(klass)`, whose body does
# `klass.extend SingletonClassMethods` — so the includer's CLASS OBJECT
# answers `instance`. No project fragment holds that name, which is why
# `Subscriber.instance` (rails actionpack) read as a conclusive miss.
# The `module Singleton` reopening below is the rails shape too
# (`activesupport/lib/active_support/core_ext/object/duplicable.rb:71`):
# it is what makes the ancestor chain RESOLVE, so the lookup reaches a
# verdict at all.
require "singleton"

module Singleton
  def sgl_duplicable?
    false
  end
end

class SglTracker
  include Singleton

  def collect
    :collected
  end
end

raise "bad" unless SglTracker.instance.collect == :collected
