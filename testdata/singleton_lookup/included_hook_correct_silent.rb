# The silence side: the dynamic `base.extend(ClassMethods)` is keyed on the
# method NAME, so a call to a name ClassMethods really defines softens to
# Inconclusive and stays silent. Fail-closed by construction — the hook's
# receiver is never proven, only the name is.
module Scopeable
  def self.included(base)
    base.extend(ClassMethods)
  end

  module ClassMethods
    def default_scope_name
      "all"
    end
  end
end

class Record
  include Scopeable
end

Record.default_scope_name
