# Bead ita-uye, the ALLOWLIST itself. The receiver here IS a known core
# class, so the core gate is satisfied — but `instance_eval` is precisely the
# method that rebinds `self`, and it is absent from
# `core_block_keeps_lexical_self`. Absent means unproven means soft.
#
# This is not a synthetic case: inside that block `self` is the String, so
# accusing `undefined method ... for BlkSelfCoreEval` would name the wrong
# class entirely — a real false positive, not merely an over-eager one.
#
# A mutation that ignores the allowlist and proves every core method lexical
# is caught by exactly this fixture and nothing else.
class BlkSelfCoreEval
  def run
    "abc".instance_eval { blkself_core_eval_missing }
  end
end
