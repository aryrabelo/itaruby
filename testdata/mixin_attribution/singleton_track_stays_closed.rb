# `extend` puts the module on the receiver's SINGLETON, so an INSTANCE
# lookup on the extender must stay accusable. The attributed-mixin family
# deliberately does not open a class for an `extend` edge (see
# `AttributedMixinEdge`): the only openness this index has is read by
# both lookups, so opening X for an `extend` edge would silence every
# instance lookup on X — a widening no fixture and no corpus asked for
# (measured 2026-09-19: the three public corpora are byte-equal with that
# arm gone).
#
# The MIRROR direction (`include` silencing a class-level call) cannot be
# pinned here, and that is a measurement: this checker emits no singleton
# `NotFound` for a project class at all (probed 2026-09-19:
# `class X; end; X.absent_name` reports nothing), so there is no verdict
# for an instance-track openness to break.
module MixAttrTrackAnswersEveryName
  def method_missing(name, *args)
    "answered #{name}"
  end

  def respond_to_missing?(_name, _include_private = false)
    true
  end
end

class MixAttrTrackExtendedBuilder
  def rakefile
    mix_attr_track_extended_absent
  end
end

MixAttrTrackExtendedBuilder.extend(MixAttrTrackAnswersEveryName)

MixAttrTrackExtendedBuilder.new.rakefile
