# frozen_string_literal: true

# Bead ita-w2c, bead F control: only SELF-sends are softened. An
# explicit-receiver call inside a rebindable block still names its own
# receiver, which `instance_eval` never touches — `helper` stays the same
# local, and `w2c_absent_step` is genuinely missing from `Helper`. MRI
# raises NoMethodError on the blamed line.
module W2cRebindableGuardExplicitReceiverAccuses
  class Part
    def initialize(&blk)
      instance_eval(&blk)
    end
  end

  class Helper; end

  class MessageBuilder
    def build
      helper = Helper.new
      Part.new do
        helper.w2c_absent_step
      end
    end
  end
end

W2cRebindableGuardExplicitReceiverAccuses::MessageBuilder.new.build