# Mechanism 2 (attribution for a non-literal receiver whose value is a
# resolvable class): the exact Rails shape. `builder_class` is a local
# assigned the return of `mix_attr_get_builder_class`, whose body is a
# ternary of two constant paths — so the mixin's receiver provably holds
# ONE of exactly those two classes, and the module (which answers every
# name through `method_missing`) is attributed to BOTH. Each opens; no
# name on either is knowable.
#
# Attributing to CADA candidate, never "some class of the program": a
# class the ternary never names keeps its accusation
# (`ternary_without_method_missing_still_accuses.rb` is the other side).
#
# MRI runs clean: `defined?(::MixAttrTernaryPluginBuilder)` is truthy, so
# the module is really mixed into `MixAttrTernaryPluginBuilder`, whose
# `assemble` forwards through `method_missing`. The else arm
# (`MixAttrTernaryAppBuilder`) is the branch MRI never takes here, and it
# is exactly the one a static reader must still attribute.
module MixAttrTernaryForwarding
  def method_missing(name, *args)
    "forwarded #{name}"
  end
end

class MixAttrTernaryPluginBuilder
  def assemble
    mix_attr_ternary_run("bundle install")
  end
end

class MixAttrTernaryAppBuilder
  def assemble
    mix_attr_ternary_template("Rakefile")
  end
end

class MixAttrTernaryGenerator
  def mix_attr_get_builder_class
    defined?(::MixAttrTernaryPluginBuilder) ? ::MixAttrTernaryPluginBuilder : MixAttrTernaryAppBuilder
  end

  def build
    builder_class = mix_attr_get_builder_class
    builder_class.include(MixAttrTernaryForwarding)
    builder_class
  end
end

built = MixAttrTernaryGenerator.new.build
raise "method_missing should answer" unless built.new.assemble == "forwarded mix_attr_ternary_run"
