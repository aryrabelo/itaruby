# typed: strict
# frozen_string_literal: true

# Bead ita-p24: genuinely malformed `#:` comments adjacent to the newly
# accepted shapes must still raise E0105 — the other half of "prova dos dois
# lados". See `rbs_comments.rs`'s `malformed_forms_still_warn_e0105`.
class RbsCommentFormsStillWarns
  #: (*String args
  def unterminated_rest_positional(*args)
  end

  #: { (String) -> void -> void
  def unterminated_block(&blk)
  end

  #: [T (String value) -> T
  def unterminated_type_params(value)
    value
  end

  #: (^Integer -> Integer) -> void
  def malformed_proc_type(callback)
    callback
  end

  #: (String x) -> [String, String
  def unterminated_tuple(x)
    [x, x]
  end

  #: (String x) -> singleton(String
  def unterminated_singleton(x)
    nil
  end
end
