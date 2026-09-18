# The discriminating case: the class IS subclassed, but no descendant
# defines the missing method either. Mirrors a real corpus shape — a class
# whose one subclass inherits the broken method and never defines the
# missing identifier. Being subclassed is not a licence to go quiet — must
# still report E0101.
class AbsHookOrphanBase
  def run
    abs_hook_missing
  end
end

class AbsHookOrphanChild < AbsHookOrphanBase
  def something_else
    42
  end
end
