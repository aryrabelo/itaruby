class IvarUnknownGadget
  def spin
    1
  end
end

class IvarUnknownWidget
  def initialize
    @gadget = IvarUnknownGadget.new
  end

  def broken
    @gadget.nonexistent_method
  end
end
