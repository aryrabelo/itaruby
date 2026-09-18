# Bead ita-ekg: same collector mechanism as class_nodoc_silent.rb, but the
# `#:doc:` directive trails a `def` line directly (not a class line) —
# an endless (one-line) method, so the very next line is itself another
# `def`, exactly the line-adjacency the collector keys on. Must stay
# silent: no E0105, no sig misattached to `public_method`.
class RdocDirDocLine
  def internal_helper(x) = x #:doc:
  def public_method(value)
    value
  end
end
