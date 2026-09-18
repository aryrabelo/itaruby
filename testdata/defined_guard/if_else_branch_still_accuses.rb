# Bead ita-r8k, control: the ELSE-branch of `if defined?(X)` runs exactly
# when `defined?` proved X does NOT exist, so a read of that constant
# there is genuinely unsafe and must still warn. Proves the suppression
# added for the then-branch (bead ita-r8k) does not leak to the opposite
# branch.
if defined?(::DefinedGuardElseUnsafe)
  1
else
  ::DefinedGuardElseUnsafe
end
