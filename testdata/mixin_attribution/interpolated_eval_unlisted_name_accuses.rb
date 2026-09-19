# Mechanism 3's control: the harvest files EXACTLY the names the literal
# list spells, never a blanket. A class that mixes the module in
# dynamically and calls a name NOT in the list gets a real E0101 — the
# harvested names soften, an unlisted name does not.
#
# MRI raises `NoMethodError` on the blamed line: the `class_eval` defines
# only `mix_attr_control_template`, so `mix_attr_control_absent` exists on
# no ancestor of the builder.
module MixAttrHarvestControlActionMethods
  %w(mix_attr_control_template).each do |method|
    class_eval <<-RUBY, __FILE__, __LINE__ + 1
      def #{method}(*args)
        "harvested #{method}"
      end
    RUBY
  end
end

class MixAttrHarvestControlBuilder
  def rakefile
    mix_attr_control_absent
  end
end

def mix_attr_control_install(builder_class)
  builder_class.include(MixAttrHarvestControlActionMethods)
end

mix_attr_control_install(MixAttrHarvestControlBuilder)
MixAttrHarvestControlBuilder.new.rakefile
