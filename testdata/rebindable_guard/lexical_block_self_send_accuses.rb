# frozen_string_literal: true

# Bead ita-w2c, bead F control: `[1, 2].each` is a core iterator whose
# block provably keeps lexical `self` (`block_keeps_lexical_self`), so a
# self-send in it stays conclusive. MRI raises NoMethodError on the
# blamed line.
module W2cRebindableGuardLexicalAccuses
  class Walker
    def run
      [1, 2].each do |i|
        w2c_absent_step(i)
      end
    end
  end
end

W2cRebindableGuardLexicalAccuses::Walker.new.run