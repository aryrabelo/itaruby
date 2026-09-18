# Bead ita-uye, the other side of that narrowing: `tap` on a KNOWN core
# receiver stays conclusive. Class identity is the proof here — we know
# exactly whose `tap` runs, and `String` was never reopened by this project —
# so the full allowlist applies and the narrowing costs nothing where the
# receiver is known.
class BlkSelfCoreTap
  def run
    "abc".tap { blkself_core_tap_missing }
  end
end
