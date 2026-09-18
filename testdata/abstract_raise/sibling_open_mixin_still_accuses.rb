# The scope control (rails Tags::SearchField shape): a SIBLING's open
# mixin must not silence a lookup it can never answer. `AbstractScopeBase`
# carries the abstract-raise idiom; `AbstractScopeSibling` (another
# descendant) includes a module opened by an unmodeled class-body call;
# the bare `request` lives in `AbstractScopeKid`'s own body, so its
# runtime dispatch set is `AbstractScopeKid` alone — clean. Expected:
# exactly one E0101 at the `request` call (24:5).
class AbstractScopeBase
  def render
    raise NotImplementedError, "subclass"
  end
end

module AbstractScopeOpenMixin
  some_unmodeled_class_body_dsl :x
end

class AbstractScopeSibling < AbstractScopeBase
  include AbstractScopeOpenMixin
end

class AbstractScopeKid < AbstractScopeBase
  def paint
    request
  end
end

AbstractScopeKid.new.paint
