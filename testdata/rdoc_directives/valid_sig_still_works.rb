# Control (bead ita-ekg): a glued-paren sig comment (`#:(Type) -> Ret`, no
# space after `#:`) is NOT an RDoc directive — `(` can never start an RDoc
# directive, and IS a valid RBS sig start. The fix must only skip glued
# WORD forms (`#:nodoc:`, `#:doc:`), never a glued `(`. The sig must still
# attach and drive real checks: wrong arg count -> E0102, wrong arg type
# -> E0103.
class RdocDirValidSigParam
end

class RdocDirValidSigOther
end

class RdocDirValidSigArity
  #:(RdocDirValidSigParam) -> void
  def accept(x)
  end
end

class RdocDirValidSigType
  #:(RdocDirValidSigParam) -> void
  def accept(x)
  end
end

RdocDirValidSigArity.new.accept(RdocDirValidSigParam.new, RdocDirValidSigParam.new)
RdocDirValidSigType.new.accept(RdocDirValidSigOther.new)
