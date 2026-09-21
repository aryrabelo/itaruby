# frozen_string_literal: true

# Bead ita-nst CONTROL: the accusation must survive — a class nested in a
# block whose method exists NOWHERE stays in the residue bucket. MRI runs
# to completion (NeverCalled is never called).
class Host
  def self.item(&block)
    block.call
  end

  item do
    class NeverCalled
    end
  end
end
