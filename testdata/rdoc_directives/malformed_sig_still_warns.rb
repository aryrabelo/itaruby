# Control (bead ita-ekg): a genuine sig ATTEMPT with a real space after
# `#:` (never glued) must keep firing E0105 when malformed — the fix only
# ever touches glued, letter-first comments, never a spaced one.
class RdocDirMalformedControl
  #: String ->
  def broken(x)
  end
end
