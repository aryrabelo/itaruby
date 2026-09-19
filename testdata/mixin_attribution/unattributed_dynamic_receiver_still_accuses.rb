# Mechanism 1, the silence side's control: the SAME `method_missing`
# module, mixed in through a receiver this index can NOT name (a bare
# method parameter — no constant path, no local assigned a constant, no
# ternary of constants). The receiver-blind form of this softening was
# measured silencing 100% of two corpora (AGENTS.md, bead ita-a8z), so an
# unnamable receiver must attribute NOTHING: `MixAttrUnattributedBuilder`
# stays closed and `mix_attr_unattributed_run` is a real E0101.
#
# MRI raises `NoMethodError` on the blamed line — the module was never
# mixed into this class, exactly as the checker reports.
module MixAttrUnattributedForwarding
  def method_missing(name, *args)
    "forwarded #{name}"
  end
end

class MixAttrUnattributedBuilder
  def rakefile
    mix_attr_unattributed_run("bundle install")
  end
end

def mix_attr_install_unattributed(builder_class)
  builder_class.include(MixAttrUnattributedForwarding)
end

MixAttrUnattributedBuilder.new.rakefile
