# Bead B, onda 2 — the shape rails' `LazyLoadHooksTest::FakeContext`
# really has.
#
# `on_load(:sym) { ... }` stores a BLOCK under a symbol; `run_load_hooks(
# :sym, Base)` hands `Base` to it, and ActiveSupport's own `execute_hook`
# runs it as `base.class_eval(&block)`
# (`activesupport/lib/active_support/lazy_load_hooks.rb:107`). So
# `wrestler` really IS an instance method of `Base` by the last line,
# and `Base` is written as a RELATIVE name from inside `Host` — exactly
# like the rails test naming its own `FakeContext`.
#
# MRI: exits 0 and prints "John Cena".
# `ita check`: silent (the base's instance surface is open, not closed).
module LazyHookSilentRegistry
  def self.on_load(name, &block)
    (@load_hooks ||= {})[name] = block
  end

  def self.run_load_hooks(name, base = Object)
    (@load_hooks ||= {})[name]&.call(base)
  end
end

class LazyHookSilentHost
  class Base
  end

  LazyHookSilentRegistry.on_load(:wrestler_hook) do |base|
    base.class_eval do
      def wrestler
        "John Cena"
      end
    end
  end

  LazyHookSilentRegistry.run_load_hooks(:wrestler_hook, Base)
end

puts LazyHookSilentHost::Base.new.wrestler