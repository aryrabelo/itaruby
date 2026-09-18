# A by-name singleton patch on a class the project does NOT declare must
# not invent it. discourse patches `TCPSocket.singleton_class`, and the
# first version of this pass interned `TCPSocket`, turning the stdlib
# class into a closed, method-less project class: one new E0101 on
# `TCPSocket.new(...).close`, code that runs. Same shape here with a
# stdlib class MRI can exercise for free.
module ClockPatch
  def at_epoch
    at(0)
  end
end

Time.singleton_class.prepend ClockPatch

raise "expected the stdlib method to still answer" unless Time.at(0).utc.year == 1970
raise "expected the patched class method" unless Time.at_epoch.utc.year == 1970
