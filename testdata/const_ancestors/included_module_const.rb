# A constant reached through an `include`d module must resolve: Ruby looks up
# the ancestors of the innermost cref after lexical nesting fails. Silent.
module ConstAncHost
  module ConstAncShared
    CONST_ANC_LIMIT = 10
  end

  class ConstAncUser
    include ConstAncShared

    def limit
      CONST_ANC_LIMIT
    end
  end
end
