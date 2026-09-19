# Bead B, onda 2 — the shape gate, half 2: the hook NAME must be a
# literal symbol at the call site. A name held in a variable is a hook
# registry this checker cannot follow, so the class it names as the base
# must stay closed and keep accusing.
#
# MRI: raises NoMethodError (`wrestler` is never installed).
# `ita check`: exactly one error, on `wrestler`.
module LazyHookNameRegistry
  def self.run_load_hooks(name, base = Object)
    name.nil? ? nil : base
  end
end

class LazyHookNameControlBase
end

hook_name = :wrestler_hook
LazyHookNameRegistry.run_load_hooks(hook_name, LazyHookNameControlBase)

LazyHookNameControlBase.new.wrestler