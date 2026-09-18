# ita-d4 fix, boundary proof: `concat` genuinely accepts ZERO arguments
# too (min_args is 0, not 1) — verified `ruby --disable-gems -e
# 'a=[1]; a.concat; p a'` => `[1]` (no error) on Ruby 3.4.2. This is the
# other half of the two-sided proof for the `(0, None)` fix: not just
# "more than 1 is fine" but "0 is fine", which a naive `(1, None)` fix
# would still have gotten wrong.
class CoreArityConcatZeroArgsSilent
  def noop
    arr = []
    arr.concat
  end
end
