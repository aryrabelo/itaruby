# G1 variant: mixed form (Ruby 3.0) — an explicit leading required
# param followed by `...`. The real required count (1) stays modeled,
# but the forwarding tail still opens the max: calling with more than
# just the one required arg must stay silent.
class FwdArgMixedForwarder
  def relay(tag, ...)
  end
end

FwdArgMixedForwarder.new.relay(:tag, 1, 2)
