# Bead B, onda 2 — the shape gate, half 1: `run_load_hooks` takes
# exactly TWO positional arguments. A third one means this is not the
# library's call at all, so the class it happens to mention second must
# stay closed and keep accusing.
#
# MRI: raises NoMethodError (`wrestler` is never installed).
# `ita check`: exactly one error, on `wrestler`.
module LazyHookArityRegistry
  def self.run_load_hooks(name, base = Object, extra = nil)
    extra.nil? ? base : nil
  end
end

class LazyHookArityControlBase
end

LazyHookArityRegistry.run_load_hooks(:wrestler_hook, LazyHookArityControlBase, 0)

LazyHookArityControlBase.new.wrestler