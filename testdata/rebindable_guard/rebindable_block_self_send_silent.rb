# frozen_string_literal: true

# Bead ita-w2c, bead F: mirrors discourse's
# `lib/email/message_builder.rb:200-203` — `body html` inside
# `Mail::Part.new do ... end`. At runtime the block's `self` is the Part
# being built, and its one-argument `body` is the setter that call
# reaches; the ENCLOSING builder's own `body` takes no argument at all.
# Resolving the call against the lexical class is therefore not evidence
# of anything. MRI: exits 0, `build` returns the Part.
module W2cRebindableGuardSilent
  class Part
    def initialize(&blk)
      instance_eval(&blk)
    end

    def body(value)
      @body = value
    end

    def body_value
      @body
    end
  end

  class MessageBuilder
    def build
      @part = Part.new do
        body "html body"
      end
    end

    def body
      @body
    end
  end
end

part = W2cRebindableGuardSilent::MessageBuilder.new.build
raise 'the block must have run against the Part being built' unless
  part.body_value == "html body"