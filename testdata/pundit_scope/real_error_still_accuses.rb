# Control: the ancestry resolves fully and closed (same shape as the
# chatwoot fixture), but `resolve` calls a method nothing in the chain
# defines. Correct superclass resolution must still let a REAL E0101
# through — the fix is never allowed to go blanket-silent on this shape.
class PunditScopeCtlApplicationPolicy
  class Scope
    attr_reader :scope, :account, :account_user
  end
end

class PunditScopeCtlDataImportPolicy < PunditScopeCtlApplicationPolicy
  class Scope < Scope
    def resolve
      scope.where(id: 1)
      totally_nonexistent_pundit_method
    end
  end
end
