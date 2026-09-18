# Bead ita-y0s MANDATORY anti-regression control: two DIFFERENT vendored
# `.rbi` files each nest a module under the SAME bare simple name
# ("Shared") — `rbi_map["Shared"]` is the UNION of both files' paths
# (bead ita-k9j.3). `own_member` names a constant that genuinely belongs
# to `RbiY0sHomonymOuterX::Shared` and must resolve silently.
# `cross_file_member_must_still_accuse` names a constant that belongs
# ONLY to the unrelated `RbiY0sHomonymOuterY::Shared` fragment reached
# through the SAME bare-name fallback key — it must keep accusing: the
# fallback may hand the query MORE candidate files, but the per-file
# exact-path refilter must still reject whichever candidate is not
# genuinely `RbiY0sHomonymOuterX::Shared`, never resolve to "the first
# file under that bare key".
class RbiY0sHomonymReader
  def own_member
    RbiY0sHomonymOuterX::Shared::X_ONLY
  end

  def cross_file_member_must_still_accuse
    RbiY0sHomonymOuterX::Shared::Y_ONLY
  end
end
