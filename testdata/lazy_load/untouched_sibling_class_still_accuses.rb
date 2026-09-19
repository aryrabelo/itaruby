# Bead B, onda 2 — the control that proves the openness is keyed to the
# base the call NAMES, not to "this file runs load hooks somewhere".
#
# `Touched` and `Untouched` are siblings in the same file, under the same
# registry and the same hook symbol; only `Touched` is handed to
# `run_load_hooks`. `Untouched.new.wrestler` is therefore a real
# `NoMethodError` and must keep being reported.
#
# MRI: raises NoMethodError on the last line (`wrestler` was installed on
# `Touched` alone). `ita check`: exactly ONE error, on `Untouched`.
module LazyHookSiblingRegistry
  def self.on_load(name, &block)
    (@load_hooks ||= {})[name] = block
  end

  def self.run_load_hooks(name, base = Object)
    (@load_hooks ||= {})[name]&.call(base)
  end
end

class LazyHookSiblingHost
  class Touched
  end

  class Untouched
  end

  LazyHookSiblingRegistry.on_load(:wrestler_hook) do |base|
    base.class_eval do
      def wrestler
        "John Cena"
      end
    end
  end

  LazyHookSiblingRegistry.run_load_hooks(:wrestler_hook, Touched)
end

puts LazyHookSiblingHost::Touched.new.wrestler

LazyHookSiblingHost::Untouched.new.wrestler