class IvarMismatchGadget
  def spin
    1
  end
end

class IvarMismatchWidget
  def initialize(flag)
    if flag
      @gadget = IvarMismatchGadget.new
    else
      # A second assignment of a different type collapses the ivar's
      # inferred type straight to Unknown — never a false positive.
      @gadget = "not a gadget"
    end
  end

  def broken
    @gadget.nonexistent_method
  end
end
