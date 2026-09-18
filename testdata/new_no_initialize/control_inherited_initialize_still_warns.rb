# Control: `initialize` defined on a PROJECT ancestor (not the
# receiver class itself) is still `MethodLookup::Found` via the
# ancestor walk, so arity checking survives inheritance — only a
# NotFound verdict (no `initialize` anywhere in the chain) is silenced.
class NoInitAncestorBase
  def initialize(x)
  end
end

class NoInitAncestorChild < NoInitAncestorBase
end

NoInitAncestorChild.new
