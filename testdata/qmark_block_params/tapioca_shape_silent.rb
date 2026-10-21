# typed: strict
# frozen_string_literal: true

# Bead ita-hfn: tapioca's own real shape
# (lib/tapioca/helpers/test/isolation.rb:30, :75) —
# `#: ?{ (?) -> untyped } -> String`, an optional block whose OWN param
# list is `(?)`, the RBS "untyped function" placeholder (block params
# carry no type information; the block's own arity is never checked
# against anything). `srb tc` accepts this verbatim. Must not warn E0105
# — and the method's OWN return type must still type its consumers (not
# degraded to Unknown along with the block): the body returns an ivar no
# one writes (Unknown), so hover reading `String` on the call proves the
# sig — see `qmark_block_params.rs`'s
# `tapioca_shape_is_silent_and_return_type_still_checked`.
class QmarkBlockTapiocaProbe
  #: ?{ (?) -> untyped } -> String
  def isolate_from_fork
    yield if block_given?
    @isolated
  end
end

# Since MATRIX H2 the `#:` sig judges only literal arguments: the probe's
# `String` result fed into an `Integer`-only sink is never judged, the
# written `"isolated"` is the one E0103.
class QmarkBlockIntegerSink
  #: (Integer) -> void
  def take(value)
  end
end

QmarkBlockIntegerSink.new.take(QmarkBlockTapiocaProbe.new.isolate_from_fork)
QmarkBlockIntegerSink.new.take("isolated")
