# Negative control for defect A: `ConstVisAlpha` is defined inside
# `ConstVisEnum`'s class-body block (block_const_def.rb), but
# `ConstVisUnrelated` neither nests in nor inherits from `ConstVisEnum`.
# Real Ruby: constant lookup is lexical plus ancestor-based, and this
# class has neither relation to `ConstVisEnum`, so `ConstVisAlpha` is
# genuinely unreachable here — a real NameError at runtime. This proves
# the block recursion the fix adds lands in the LEXICALLY ENCLOSING class
# (`ConstVisEnum`), not at top level or project-wide. Must STILL WARN
# E0104.
class ConstVisUnrelated
  def wrong
    ConstVisAlpha
  end
end
