module MixinRegistry
  def self.lookup
    Module.new
  end
end

class ImplicitDynamicMixin
  include MixinRegistry.lookup

  def use_it
    dynamic_only_method
  end
end
