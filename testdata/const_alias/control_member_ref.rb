# Bead ita-47y control: both must accuse — `ConstAlias47yControlBridge`
# resolving through the alias must NEVER suppress a genuinely wrong
# member reached through it.
class ConstAlias47yControlReader
  def bad_const
    ConstAlias47yControlBridge::Real::NoSuchConst
  end

  def bad_method
    ConstAlias47yControlBridge::Real.new.no_such_method
  end
end
