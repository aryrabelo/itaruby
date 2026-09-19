# The DEFINITION half of the cross-file pair: the module that answers
# every name, and the generator method whose return names the builder —
# both written in a different file than the mixin call site that consumes
# them (`attributed_receiver_across_files_call_site.rb`). Phase 2 resolves
# `MixinReceiver::Call` against `const_returning_methods` only AFTER every
# file is merged, which is the whole reason that map is keyed by method
# NAME and not by the call site's file (rails'
# `builder_class.include(ActionMethods)` holds a value whose producer
# lives elsewhere).
#
# Both halves reopen the SAME class, so the pair is MRI-executable: the
# call site's receiverless call really resolves at runtime.
module MixAttrXfileForwarding
  def method_missing(name, *args)
    "answered #{name}"
  end

  def respond_to_missing?(_name, _include_private = false)
    true
  end
end

class MixAttrXfileGenerator
  def mix_attr_xfile_builder_class
    MixAttrXfileBuilder
  end
end
