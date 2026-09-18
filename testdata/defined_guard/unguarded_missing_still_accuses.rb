# Bead ita-r8k, control: a plain unguarded reference to a constant that
# exists nowhere in the project must still warn. Sanity check every other
# test in this fixture set depends on: if this ever goes silent, the fix
# has degraded into blanket suppression instead of a guard-scoped one.
::DefinedGuardMissing
