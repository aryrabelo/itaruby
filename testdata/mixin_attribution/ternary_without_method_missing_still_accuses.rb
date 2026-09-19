# Mechanism 2's method_missing gate, control side: the receiver IS
# attributed (a ternary of constant paths, exactly the silent fixture's
# shape), but the mixed-in module answers NO `method_missing` — so the
# openness rule must NOT fire, and a name neither the module defines nor
# forwards is a real E0101. An attributed edge is not by itself a licence
# to open a class; only a module that answers every name is.
#
# MRI raises `NoMethodError` on the blamed line: the module is mixed in
# but defines only `mix_attr_gated_provided`, and nothing answers
# `mix_attr_gated_absent`.
module MixAttrGatedMixin
  def mix_attr_gated_provided
    "ok"
  end
end

class MixAttrGatedPluginBuilder
  def assemble
    mix_attr_gated_absent
  end
end

class MixAttrGatedAppBuilder
  def assemble
    mix_attr_gated_absent
  end
end

class MixAttrGatedGenerator
  def mix_attr_gated_class
    defined?(::MixAttrGatedPluginBuilder) ? ::MixAttrGatedPluginBuilder : MixAttrGatedAppBuilder
  end

  def build
    builder_class = mix_attr_gated_class
    builder_class.include(MixAttrGatedMixin)
    builder_class
  end
end

MixAttrGatedGenerator.new.build.new.assemble
