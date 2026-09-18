class SuperBase
  def greet
    'base'
  end
end

class SuperChild < SuperBase
  def greet
    "child: #{super}"
  end
end
