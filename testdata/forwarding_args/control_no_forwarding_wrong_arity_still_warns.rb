# G2 negative control: a method with NO `...` keeps its real, fixed
# arity check — the forwarding fix must not widen ordinary methods.
class FwdArgControlNoForwarding
  def relay(a, b)
  end
end

FwdArgControlNoForwarding.new.relay(1, 2, 3)
