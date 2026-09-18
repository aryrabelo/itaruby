# Bead ita-r8k: the `unless` mirror of the ternary rails shape.
# `unless defined?(::X); Y; else; ::X; end` only reaches the else-clause
# when `defined?` proved `::DefinedGuardExtUnless` exists, so the read of
# that same constant there must stay silent — same fact as the `if`/
# ternary form, opposite branch.
module DefinedGuardUnlessNs
  DefinedGuardUnlessDefault = "real"
end

klass = unless defined?(::DefinedGuardExtUnless)
  DefinedGuardUnlessNs::DefinedGuardUnlessDefault
else
  ::DefinedGuardExtUnless
end
puts klass
