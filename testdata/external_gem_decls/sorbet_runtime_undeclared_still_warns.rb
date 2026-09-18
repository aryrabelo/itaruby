# Control for the 2026-08-26 T::* entries (mirrors still_unresolved.rb):
# names OUTSIDE the curated sorbet-runtime namespace list must still warn
# E0104 — the entries are an allowlist, never a T::-prefix suppressor.
class TSRubyControl
  #: (TSRubyControl) -> void
  def wrap(other)
    T::NotACuratedName
  end
end
