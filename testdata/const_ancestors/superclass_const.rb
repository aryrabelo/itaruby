# A constant defined on the superclass must resolve from the subclass body.
# Silent.
module ConstAncSup
  class ConstAncBase
    CONST_ANC_PATTERN = /x/
  end

  class ConstAncChild < ConstAncBase
    def pattern
      CONST_ANC_PATTERN
    end
  end
end
