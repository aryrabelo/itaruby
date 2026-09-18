class Greeter
  #: (Integer) -> String
  def greet(n)
    n.to_s
  end

  def echo(v)
    v
  end
end

Greeter.new.greet("str")

untyped_value = Greeter.new.echo(42)
Greeter.new.greet(untyped_value)
