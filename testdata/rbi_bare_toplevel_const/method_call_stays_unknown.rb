# Bead ita-h5s: `rbi_toplevel_const_declares` is suppression-only
# (invariant #1) — a hit silences E0104 but NEVER interns `XClient` into
# `ProjectIndex`, so `infer_const` still types the reference
# `Ty::Unknown`. Calling an arbitrary, undefined-anywhere method on it
# must never emit E0101 (`Ty::Unknown` never diagnoses).
class Rbih5sMethodCallControl
  def call_unknown
    XClient.nonexistent_method_xyz
  end
end
