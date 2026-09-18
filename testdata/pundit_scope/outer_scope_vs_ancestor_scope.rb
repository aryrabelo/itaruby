# Precedence fixture: nesting and ancestors DISAGREE on what `Scope`
# means. A top-level `Scope` (deliberately UNPREFIXED — the whole point
# is a name collision with the bare `Scope` Pundit always writes) is
# unrelated and defines nothing. The REAL answer is reached only via
# `PunditScopeVsDataImportPolicy`'s superclass ancestry. A resolver that
# falls back to the outermost lexical/top-level `Scope` before trying
# the real ancestor chain lands on the wrong (empty) class, and
# `scope.where(...)` — self-send calling `scope` — would wrongly
# conclude E0101. The ancestor answer must win.
class Scope
end

class PunditScopeVsApplicationPolicy
  class Scope
    attr_reader :scope, :account, :account_user
  end
end

class PunditScopeVsDataImportPolicy < PunditScopeVsApplicationPolicy
  class Scope < Scope
    def resolve
      scope.where(id: 1)
    end
  end
end
