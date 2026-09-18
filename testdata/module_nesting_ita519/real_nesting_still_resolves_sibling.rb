module Ita519Real
  class Ita519Helper
    def helper_method; end
  end

  # Genuinely nested via separate `module`/`class` keywords — Module.nesting
  # here is TWO real levels: [Ita519Real::UsesRealNesting, Ita519Real]. The
  # fix must not collapse this to a single level: `Ita519Helper` resolves
  # through the outer `Ita519Real` nesting level exactly as before.
  class UsesRealNesting
    def call
      Ita519Helper.new.helper_method
    end
  end
end
