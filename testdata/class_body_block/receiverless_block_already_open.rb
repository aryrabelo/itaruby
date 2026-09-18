# Documents the EXISTING behavior the fix extends: a class-body block on a
# RECEIVERLESS call (`included do ... end`, the ActiveSupport::Concern
# shape) already opens the class today (bead ita-o1n's ClassBodyBlock), so
# the self-send inside it and the lookup below are already Inconclusive.
# Pinned side by side with the receiver-ful loop so both shapes stay
# covered. Expected: ZERO diagnostics.
class ReceiverlessBlockAlreadyOpen
  included do
    def helper
      1
    end
  end

  def render_thing
    foo
  end
end

ReceiverlessBlockAlreadyOpen.new(nil).render_thing
