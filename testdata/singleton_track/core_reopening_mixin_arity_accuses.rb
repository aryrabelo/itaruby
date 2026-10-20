# frozen_string_literal: true

# 2026-10-19, ACCUSATION side: the mixin a `Module` reopening includes
# carries a REAL signature, so resolving through it is checkable
# knowledge and not merely silence. `mrm_need` takes one argument; the
# zero-argument call on the class object is an E0102 MRI really raises.
module MrmNeeds
  def mrm_need(width)
    width
  end
end

class Module
  include MrmNeeds
end

class MrmBoard; end

MrmBoard.mrm_need
