# The false positive this rule exists to kill. `AbsHookBase#run` self-sends
# `abs_hook_host`, which the base never defines — but the only class ever
# instantiated is the subclass, which does define it. A self-send dispatches
# on the runtime class, so this resolves. Must be SILENT.
class AbsHookBase
  def run
    connect(abs_hook_host)
  end

  def connect(target)
    target
  end
end

class AbsHookChild < AbsHookBase
  private

  def abs_hook_host
    "sftp.example.com"
  end
end
