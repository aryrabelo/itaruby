# G1 (ita-7g9): `def foo(...)` (Ruby 3.0 argument forwarding,
# ForwardingArgumentsNode/ForwardingParameterNode in prism) forwards
# whatever the caller passes through — positional, keyword, and block —
# so its arity is open: no cap, no minimum. Calling with 0, 1, and 3
# positional args must all stay silent (no E0102).
class FwdArgForwarder
  def relay(...)
  end
end

f = FwdArgForwarder.new
f.relay
f.relay(1)
f.relay(1, 2, 3)
