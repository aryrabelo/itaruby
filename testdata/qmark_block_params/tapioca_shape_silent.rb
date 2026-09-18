# typed: strict
# frozen_string_literal: true

# Bead ita-hfn: tapioca's own real shape
# (lib/tapioca/helpers/test/isolation.rb:30, :75) —
# `#: ?{ (?) -> untyped } -> String`, an optional block whose OWN param
# list is `(?)`, the RBS "untyped function" placeholder (block params
# carry no type information; the block's own arity is never checked
# against anything). `srb tc` accepts this verbatim. Must not warn E0105
# — and the method's OWN return type must still be checked (not degraded
# to Unknown along with the block) — see `qmark_block_params.rs`'s
# `tapioca_shape_is_silent_and_return_type_still_checked`.
class QmarkBlockTapiocaProbe
  #: ?{ (?) -> untyped } -> String
  def isolate_from_fork
    yield if block_given?
    "isolated"
  end
end

# The probe's return type (`String`) fed into a sink that only accepts
# `Integer` — a genuine E0103 that proves `isolate_from_fork`'s `-> String`
# survived the `(?)` block fix intact, rather than the whole sig degrading
# to Unknown (which invariant #1 would make silently swallow this mismatch
# too).
class QmarkBlockIntegerSink
  #: (Integer) -> void
  def take(value)
  end
end

QmarkBlockIntegerSink.new.take(QmarkBlockTapiocaProbe.new.isolate_from_fork)
