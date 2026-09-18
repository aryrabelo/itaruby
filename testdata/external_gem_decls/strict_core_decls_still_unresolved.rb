# Control fixture for bead ita-083: a constant that is neither project-
# defined nor in the curated gem declarations must still warn E0104 — the
# declaration file resolves an allowlist, not "every constant" (same
# invariant `still_unresolved.rb` proves for ita-3gs's own entries).
class StrictCoreDeclNonexistentCaller
  def call
    StrictCoreDeclNonexistent.do_something
  end
end
