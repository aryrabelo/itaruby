# A module that calls the Concern DSL's block form WITHOUT the concern
# edge: the block executes as an ordinary method here (the module
# defines its own `class_methods`), so MRI runs the file to completion —
# but the harvest must not invent `Plain::ClassMethods` from it, because
# nothing says this is ActiveSupport::Concern's DSL. The checker keeps
# the module OPEN (the `_` catch-all) and invents no surface.
module Plain
  def self.class_methods(&block)
    @called = true
  end

  class_methods do
    def helper
      :invented
    end
  end
end

raise "block must have run" unless Plain.instance_variable_get(:@called)
