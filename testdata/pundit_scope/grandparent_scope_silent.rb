# Nested variant: the base `Scope` lives on a GRANDPARENT policy, not the
# direct superclass. `PunditScopeGrandChildPolicy`'s ancestry is
# [self, PunditScopeParentPolicy, PunditScopeGrandparentPolicy]; only the
# last one defines `Scope`. Resolution must climb the whole ancestor
# chain (nearest first), not stop at the immediate parent.
class PunditScopeGrandparentPolicy
  class Scope
    attr_reader :scope, :account, :account_user

    def initialize(account_user, scope)
      @account_user = account_user
      @account = account_user
      @scope = scope
    end
  end
end

class PunditScopeParentPolicy < PunditScopeGrandparentPolicy
end

class PunditScopeGrandChildPolicy < PunditScopeParentPolicy
  class Scope < Scope
    def resolve
      return scope.where(account_id: account.id) if account_user.administrator?
      scope.none
    end
  end
end
