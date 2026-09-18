# Bead ita-uye, the ASYMMETRY, and the counter-example that justifies it.
# `BlkSelfOwnEach` defines its own `each` and `instance_exec`s the block —
# the hand-rolled rebinding iterator that the Enumerable argument assumes
# nobody writes.
#
# With an UNKNOWN receiver we trust the name `each`, because we cannot see
# the callee and the protocol says it yields. Here the receiver is a known
# project class, so we CAN see that this `each` rebinds: knowing more must
# never license concluding more, so a resolvable project receiver stays
# unproven and this must be silent. `blkself_own_each_ctx` really does exist
# on the context object at runtime, so accusing here would be a plain false
# positive.
#
# This fixture is the only thing pinning that arm: every other project
# receiver in this directory uses a name outside the allowlist, so the
# allowlist guard returns before the receiver arm is ever consulted.
class BlkSelfOwnEach
  def each(&blk)
    [1, 2].each { |x| instance_exec(x, &blk) }
  end

  def blkself_own_each_ctx
    1
  end
end

class BlkSelfOwnEachUser
  def run
    coll = BlkSelfOwnEach.new
    coll.each { blkself_own_each_ctx }
  end
end
