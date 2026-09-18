# Bead ita-j0z: real ruby-lsp shape (`test/addon_test.rb:149`) —
# `x = <call> #: as untyped` on the ASSIGNMENT's own line, not a call
# argument. Calling an otherwise-unknown method on the untyped-cast
# local must produce zero diagnostics: the settings mixin is explicitly
# erased so `.settings` (a method `InlineCastUntypedAssignAddon` never
# declares) must never fire E0101.
class InlineCastUntypedAssignAddon
end

def inline_cast_untyped_assignment_silences
  addon = InlineCastUntypedAssignAddon.new #: as untyped
  addon.settings
end
