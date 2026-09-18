# Negative control for bead ita-o8l.2 / ita-o8l.3: an invented method name
# on the SAME closed-project-class shapes as `silence.rb` (bare self-send
# and explicit receiver falling back to Object) must still accuse E0101 --
# proves the fix is a narrow five-name allowlist, not a blanket
# Kernel/Object suppressor.

class O8lCoreExtControlWidget
  def bogus_bare_call
    o8l_core_ext_totally_unknown_bare_call
  end
end

class O8lCoreExtControlProbe
  def bogus_receiver_call
    O8lCoreExtControlWidget.new.o8l_core_ext_totally_unknown_receiver_call
  end
end
