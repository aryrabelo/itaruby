# Bead ita-547's `apply_gem_reopenings` alone would miss this: `activesupport`
# IS declared in this directory's `Gemfile.lock`, but the plain camelize
# heuristic (`discovery.rs`'s `gem_namespace`) guesses `Activesupport`, not
# the real `ActiveSupport` spelling — no override entry covers it, so an
# EXACT string match on the top-level path segment never fires here.
#
# Bead ita-c8h's complementary `apply_undeclared_namespace_reopenings`
# closes that gap by construction, independent of any gem-name guess: this
# project never wraps `ActiveSupport` bare (no `module ActiveSupport; end`
# anywhere), so the reopening below is still recognized as external and
# stays silent — the two mechanisms are additive, and together they cover
# what neither covers alone. See `gem_superclass_reopen.rs` for the
# Gemfile.lock-free fixtures that isolate ita-c8h's own mechanism.
class ActiveSupport::GemReopenLockHelper
  def real_method
    1
  end
end

ActiveSupport::GemReopenLockHelper.new.nonexistent_method
