module Registry
  module Helpers
    def self.mixin
      Module.new
    end
  end
end

module Wrapper
  def self.unsafe(obj)
    obj
  end
end

class DynamicExtendTarget
  Wrapper.unsafe(self).extend Registry::Helpers.mixin

  def self.use_it
    dynamic_helper_method
  end
end
