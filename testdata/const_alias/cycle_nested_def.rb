# Bead ita-47y: a cyclic alias reached via a MULTI-segment reference —
# `resolve_const_through_aliases`'s own first-segment resolution
# delegates to `resolve_const_via_alias`'s existing cycle guard, so this
# must never hang and must never fabricate a suppression for a reference
# that genuinely resolves nowhere (`resolve_alias_segment` is never even
# reached: the first segment already fails to resolve).
ConstAlias47yCycleA = ConstAlias47yCycleB
ConstAlias47yCycleB = ConstAlias47yCycleA
