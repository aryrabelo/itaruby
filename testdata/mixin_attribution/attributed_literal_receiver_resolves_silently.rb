# Mechanism 1 (attributed mixin edge, method_missing follows the
# receiver's ancestry): `MixAttrBuilder.include(MixAttrForwarding)` names
# its receiver with a LITERAL constant path, OUTSIDE any class body — the
# `DefWalker` class-body mixin arm never sees it (frag_idx is None at
# toplevel), and `note_dynamic_mixin` skips it (the receiver is a
# constant, not the local-variable shape it collects). Before this
# mechanism the edge was invisible and `mix_attr_run` was a fabricated
# E0101. The module answers every name through `method_missing`, so the
# receiver is `open`: no name on it is knowable, and the call is silent.
#
# MRI runs this file clean: `mix_attr_run` is really answered by
# `MixAttrForwarding#method_missing`.
module MixAttrForwarding
  def method_missing(name, *args)
    "forwarded #{name}"
  end

  def respond_to_missing?(_name, _include_private = false)
    true
  end
end

class MixAttrBuilder
  def rakefile
    mix_attr_run("bundle install")
  end
end

MixAttrBuilder.include(MixAttrForwarding)

raise "method_missing should answer" unless MixAttrBuilder.new.rakefile == "forwarded mix_attr_run"
