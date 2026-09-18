# Negative control: the ancestor walk is an ancestor walk, not a
# project-wide "does this name exist anywhere" search. `ConstAncOutsider`
# does not include `ConstAncSibling`, so `CONST_ANC_SECRET` is genuinely
# unreachable from here and must still warn E0104.
module ConstAncNeg
  module ConstAncSibling
    CONST_ANC_SECRET = 1
  end

  class ConstAncOutsider
    def secret
      CONST_ANC_SECRET
    end
  end
end
