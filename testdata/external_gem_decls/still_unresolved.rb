# Control fixture for bead ita-3gs: a constant that is neither project-
# defined nor in the curated gem declarations must still warn E0104 — the
# declaration file resolves an allowlist, not "every constant".
class StillUnresolvedModel
  def call
    TotallyMadeUpGemNamespace.do_something
  end
end
