# The namespace shape of namespace_only_module_stays_open.rb, but this tree
# also writes methods of the module's own, so it IS where the module's
# surface comes from and a miss on it is a certain NoMethodError.
# `Ledgerline` defines a module method; `Vaultbox` an instance method, which
# the module object itself does not answer. CO-W drops the surface check and
# must silence 25:12 and 26:10.
module Ledgerline
  def self.entries
    []
  end

  class Entry
  end
end

module Vaultbox
  def seal(item)
    item
  end

  class Lock
  end
end

Ledgerline.entires
Vaultbox.seal(:x)
