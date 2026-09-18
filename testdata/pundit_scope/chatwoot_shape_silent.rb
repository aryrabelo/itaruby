# Bead ita-t6m: the real chatwoot shape (app/policies/data_import_policy.rb).
# `class Scope < Scope` inside `PunditScopeDataImportPolicy` must resolve
# the RHS `Scope` to `PunditScopeApplicationPolicy::Scope` — reached
# through `PunditScopeDataImportPolicy`'s SUPERCLASS ancestry, not through
# lexical nesting (the two `Scope` classes are not lexically related at
# all). Every `scope`/`account`/`account_user` self-send inside `resolve`
# must stay silent.
class PunditScopeApplicationPolicy
  attr_reader :account_user, :account

  def initialize(account_user, account)
    @account_user = account_user
    @account = account
  end

  class Scope
    attr_reader :scope, :account, :account_user

    def initialize(account_user, scope)
      @account_user = account_user
      @account = account_user.account
      @scope = scope
    end

    def resolve
      raise NoMethodError, "You must define #resolve in #{self.class}"
    end
  end
end

class PunditScopeDataImportPolicy < PunditScopeApplicationPolicy
  class Scope < Scope
    def resolve
      return scope.where(account_id: account.id) if account_user.administrator?
      scope.none
    end
  end
end
