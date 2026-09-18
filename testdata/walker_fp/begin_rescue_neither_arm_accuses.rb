# Negative control for bead ita-o8l.5: a method defined in NEITHER the
# `begin` nor the `rescue` arm must still accuse — proves the fix walks
# exactly the arms present, never opens the whole class as a side effect.
class WalkerFpBeginNeitherArm
  begin
    def walker_fp_neither_begin
      1
    end
  rescue
    def walker_fp_neither_rescue
      0
    end
  end
end

WalkerFpBeginNeitherArm.new.walker_fp_totally_undefined_neither_arm
