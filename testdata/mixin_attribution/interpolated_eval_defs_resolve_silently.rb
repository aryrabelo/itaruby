# Mechanism 3 (harvest of interpolated `def` names): a literal list
# crossed with a `class_eval` heredoc that interpolates the loop
# parameter defines exactly those names — and nothing else in this walker
# can see a `def` written inside a string. Rails' `Rails::ActionMethods`
# is this shape (nine forwarding methods, 95 measured baseline sites).
# The names are FILED on the module, so a class that mixes it in
# dynamically resolves them by NAME through `dynamic_mixin_covers`, while
# any OTHER name still accuses.
#
# The receiver of the mixin here is a bare method parameter — unnamable,
# so mechanism 2 cannot attribute it and this is purely the harvest at
# work.
#
# MRI runs clean: the `class_eval` really defines these methods, so
# `mix_attr_harvest_template` answers once the module is mixed in.
module MixAttrHarvestActionMethods
  %w(mix_attr_harvest_template mix_attr_harvest_copy_file).each do |method|
    class_eval <<-RUBY, __FILE__, __LINE__ + 1
      def #{method}(*args)
        "harvested #{method} " + args.join(",")
      end
    RUBY
  end
end

class MixAttrHarvestBuilder
  def rakefile
    mix_attr_harvest_template("Rakefile")
  end
end

def mix_attr_harvest_install(builder_class)
  builder_class.include(MixAttrHarvestActionMethods)
end

mix_attr_harvest_install(MixAttrHarvestBuilder)
raise "harvest should answer" unless MixAttrHarvestBuilder.new.rakefile == "harvested mix_attr_harvest_template Rakefile"
