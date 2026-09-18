# Bead ita-r8k, rails shape (2 measured sites): `defined?(::X) ? ::X :
# Rails::X` — `::DefinedGuardExtTernary` is never defined anywhere in this
# project, but the ternary's THEN-branch only ever runs once `defined?`
# already proved it exists at runtime, so reading it there is safe. The
# ELSE-branch fallback resolves through the ordinary project index and
# must stay silent for the usual reason, unrelated to this bead's fix.
module DefinedGuardTernaryNs
  DefinedGuardTernaryDefault = "real"
end

klass = defined?(::DefinedGuardExtTernary) ? ::DefinedGuardExtTernary : DefinedGuardTernaryNs::DefinedGuardTernaryDefault
puts klass
