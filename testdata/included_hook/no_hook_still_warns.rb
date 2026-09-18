# Bead ita-1yw negative control: a module with NO hook at all — a call the
# includer genuinely cannot answer MUST keep accusing E0101.
module IncHookPlain
end

class IncHookPlainUser
  include IncHookPlain

  def go
    missing_thing
  end
end
