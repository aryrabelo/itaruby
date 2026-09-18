# ita-d4 fix: Array#concat(*other_arrays) is varargs since Ruby 2.4, not
# fixed at 1 argument. Verified `ruby --disable-gems -e 'a=[1];
# a.concat([2],[3]); p a; p Array.instance_method(:concat).arity'` =>
# `[1, 2, 3]` and `-1` on Ruby 3.4.2. The old `core.rs` entry grouped
# `concat` with `+`/`-` at `(1, Some(1))`, which produced a real false
# E0102 on the ruby-lsp FP site (requests/diagnostics.rb:33):
# `diagnostics.concat(syntax_error_diagnostics, syntax_warning_diagnostics)`.
# Calling concat with 2 array arguments must stay silent.
class CoreArityConcatTwoArgsSilent
  def merge(a, b)
    diagnostics = []
    diagnostics.concat(a, b)
  end
end
