# Precision is preserved for a leaf class: nothing subclasses it, so there
# is no runtime class other than this one and the self-send genuinely
# cannot resolve. Must still report E0101.
class AbsHookLeaf
  def run
    abs_hook_absent
  end
end
