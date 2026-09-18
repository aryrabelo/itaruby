# Bead ita-uye, UNKNOWN RECEIVER. `rows` is a parameter, so the receiver is
# `Ty::Unknown` — proof here can only be class IDENTITY, and an unknown
# receiver has none. `each` carries no evidence by itself; a class defining
# its own `each` could freely `instance_exec` the block, and with an unknown
# receiver we cannot see whether it does. So this stays silent: a deliberate
# false negative under invariant #1.
#
# A `Ty::Unknown` arm that trusted the NAME alone (the Enumerable/iteration
# protocol's published contract) was built, measured, and removed by review
# decision — argument, not proof. This fixture is the regression lock: a
# mutant that lets that arm sneak back in, in any form, must be caught here.
class BlkSelfUnknownEach
  def run(rows)
    rows.each { blkself_unknown_each_missing }
  end
end
