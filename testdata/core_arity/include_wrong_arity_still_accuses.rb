# Control: Array#include? is unrelated to this fix and stays pinned to
# exactly 1 argument — verified `ruby --disable-gems -e 'p
# Array.instance_method(:include?).arity'` => `1`, and `[1].include?(1,
# 2)` raises `ArgumentError: wrong number of arguments (given 2, expected
# 1)` on Ruby 3.4.2. Calling it with 2 args must still accuse E0102 —
# proves the general core-arity mechanism (not just this bead's
# `concat`/`+`/`-` arm) keeps working.
class CoreArityIncludeWrongArityAccuses
  def check
    arr = [1]
    arr.include?(1, 2)
  end
end
