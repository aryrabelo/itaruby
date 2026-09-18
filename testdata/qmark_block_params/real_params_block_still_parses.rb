# typed: strict
# frozen_string_literal: true

# Bead ita-hfn regression: a block with REAL (non-`(?)`) params must keep
# parsing exactly as before `block_param_list` was introduced — `(?)` is a
# new alternative shape, never a replacement for the ordinary param list.
# See `qmark_block_params.rs`'s `real_params_block_still_silent`.
class QmarkBlockRealParamsProbe
  #: (String? query) ?{ (String) -> untyped } -> Array[String]
  def scan(query)
    []
  end
end
