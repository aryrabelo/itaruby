# typed: strict
# frozen_string_literal: true

# Bead ita-hfn control ("prova dos dois lados"): `(?)` written as the
# METHOD's OWN top-level param list, not inside a block/proc clause.
# `rbs_comment.rs`'s module doc documents this precisely: the RBS spec
# (github.com/ruby/rbs docs/syntax.md's `_method-type_` production) ALSO
# defines `(?) -> T` as valid at this position ("method type with untyped
# parameters") — so this is not a spec violation on Sorbet's part. It is,
# however, deliberately OUT OF SCOPE for ita-hfn: no measured corpus
# occurrence of this shape (as opposed to the block-type shape tapioca
# uses) has turned up, so this parser still rejects it today as a real,
# if currently unmeasured, scope gap — not a false positive this parser
# invents. This fixture pins that boundary: the fix must live in
# `block_param_list` (block-clause-only), never leak into the shared
# `param_list` a top-level sig also calls. See `qmark_block_params.rs`'s
# `qmark_outside_block_position_still_warns`.
class QmarkBlockOutsidePositionStillWarns
  #: (?) -> String
  def bogus
    ""
  end
end
