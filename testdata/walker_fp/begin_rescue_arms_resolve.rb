# Bead ita-o8l.5: a `def` inside a class-body `begin`/`rescue`/`else`/
# `ensure` must resolve exactly like the `if`/`else` sibling mechanism
# already does — the real corpus site (activesupport's
# `Notifications::Instrumenter`) defines `now_cpu` once in `begin`, once
# in `rescue`. One distinct method name per arm here so each arm's own
# coverage is independently provable, not just "the name exists somewhere".
class WalkerFpBeginArms
  begin
    def walker_fp_in_begin_arm
      1
    end
  rescue
    def walker_fp_in_rescue_arm
      0
    end
  else
    def walker_fp_in_else_arm
      2
    end
  ensure
    def walker_fp_in_ensure_arm
      3
    end
  end
end

WalkerFpBeginArms.new.walker_fp_in_begin_arm
WalkerFpBeginArms.new.walker_fp_in_rescue_arm
WalkerFpBeginArms.new.walker_fp_in_else_arm
WalkerFpBeginArms.new.walker_fp_in_ensure_arm
