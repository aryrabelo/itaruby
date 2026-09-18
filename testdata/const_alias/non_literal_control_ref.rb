# Bead ita-47y control: neither `ConstAlias47yDynamicCall` nor
# `ConstAlias47yLiteralNumber` is an alias — a `::`-suffixed reference
# through either must keep warning E0104, exactly as before this bead.
class ConstAlias47yNonLiteralReader
  def via_call
    ConstAlias47yDynamicCall::SOMETHING
  end

  def via_number
    ConstAlias47yLiteralNumber::SOMETHING
  end
end
