# ita-2ve closed-world, FIRES: `x.is_a?(String)` narrows x to String
# (Ty::Str), `pushh` is in neither the allowlist nor the generated core
# inventory, no Gemfile upward of testdata/, and String is never reopened
# in this project -> E0101 (the p5 A/B gap).
class CoreConclusiveAppender
  def append(x)
    if x.is_a?(String)
      x.pushh(1)
    end
    x
  end
end
