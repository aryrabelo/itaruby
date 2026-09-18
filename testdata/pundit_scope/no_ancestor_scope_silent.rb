# Control: `class Scope < Scope` where NO ancestor anywhere defines a
# `Scope` of its own. The superclass name must stay UNRESOLVED (the
# ancestry stays open) — never resolve to something wrong, and never
# claim NotFound (invariant #1: Ty::Unknown/open ancestry never
# produces a diagnostic).
class PunditScopeOrphanBasePolicy
end

class PunditScopeOrphanChildPolicy < PunditScopeOrphanBasePolicy
  class Scope < Scope
    def resolve
      scope.where(id: 1)
    end
  end
end
