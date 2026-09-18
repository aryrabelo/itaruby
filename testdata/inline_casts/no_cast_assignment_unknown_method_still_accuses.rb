# Bead ita-j0z control: the same assignment shape with NO cast comment
# at all must still accuse E0101 on the unknown method — proves the new
# assignment-cast wiring didn't silence unrelated unknown-method calls.
class InlineCastUntypedAssignControlAddon
end

def inline_cast_untyped_assignment_control_still_accuses
  addon = InlineCastUntypedAssignControlAddon.new
  addon.settings
end
