# Bead ita-gjb: a project class with NO `initialize` anywhere in its
# ancestry (fully closed — no reopening, no dynamic superclass, nothing
# open) used to be treated as "obviously" `Object#initialize`, 0-arg,
# and any call passing arguments was flagged E0102. That assumption is
# unsound in general (most real-world classes without a project-visible
# `initialize` — core classes like Time/IPAddr, gems like FastImage —
# actually take arguments in C code this checker never models), so this
# must stay silent.
class NoInitWidget
end

NoInitWidget.new(1, 2)
