# Control: unlike `concat`, Array#+ stays pinned to EXACTLY 1 argument —
# verified `ruby --disable-gems -e 'p Array.instance_method(:+).arity'`
# => `1` on Ruby 3.4.2 (a real binary operator, never variadic). Calling
# it with 0 args must still accuse E0102. Guards against the mutant where
# fixing `concat`'s arity over-widens its match-arm sibling `+`/`-` to
# `(0, None)` too instead of splitting them apart.
class CoreArityPlusWrongArityAccuses
  def bad
    arr = [1]
    arr.+()
  end
end
