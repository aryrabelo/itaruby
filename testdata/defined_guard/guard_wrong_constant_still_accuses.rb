# Bead ita-r8k, control: `defined?(A) ? B : nil` — the predicate proves A
# exists, not B. A read of the DIFFERENT constant B in the then-branch
# must still warn: the suppression is keyed on the exact guarded path,
# never "some defined? guard was present".
result = defined?(::DefinedGuardA) ? ::DefinedGuardB : nil
puts result
