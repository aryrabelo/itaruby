# Control (bead ita-547 gate G2): a class this project itself owns, whose
# top-level name matches none of this directory's `Gemfile.lock` gems,
# must keep accusing exactly as before — the lockfile mechanism only opens
# classes under a MAPPED gem namespace, never blankets project code.
class GemReopenLockControl
  def real_method
    1
  end
end

GemReopenLockControl.new.nonexistent_method
