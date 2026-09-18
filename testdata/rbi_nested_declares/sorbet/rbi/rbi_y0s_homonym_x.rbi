# typed: true
# DO NOT EDIT MANUALLY — bead ita-y0s MANDATORY anti-regression control:
# a module nested under the bare simple name "Shared", in a DIFFERENT
# file from `rbi_y0s_homonym_y.rbi`'s own unrelated "Shared" nesting.
# `rbi_map["Shared"]` unions both files' paths (bead ita-k9j.3's
# contract) — the bare-name fallback must never let a query for THIS
# file's owner accidentally answer from the OTHER file's fragment.

module RbiY0sHomonymOuterX
  module Shared
    X_ONLY = T.let(1, Integer)
  end
end
