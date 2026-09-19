# The CALL-SITE half of the cross-file pair: the builder class and the
# method that consumes the generator's return live here, while the method
# that PRODUCES it (`mix_attr_xfile_builder_class`) and the module the
# builder mixes in live in
# `attributed_receiver_across_files_definition.rb`. Silent only if the
# phase-2 lookup really is project-wide: a lookup scoped to this file
# would leave the builder closed and accuse `mix_attr_xfile_absent`,
# which MRI answers through the mixed-in `method_missing`.
class MixAttrXfileBuilder
  def rakefile
    mix_attr_xfile_absent
  end
end

class MixAttrXfileGenerator
  def build
    builder_class = mix_attr_xfile_builder_class
    builder_class.include(MixAttrXfileForwarding)
    builder_class
  end
end

raise "cross-file builder should answer" unless MixAttrXfileGenerator.new.build.new.rakefile == "answered mix_attr_xfile_absent"
