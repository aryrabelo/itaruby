# Bead ita-1yw silence fixture: the literal hook shape
# `def self.included(base); base.attr_accessor :title; end` defines
# `title`/`title=` on every includer — reading and writing them must NOT
# diagnose (E0101).
module IncHookAttrs
  def self.included(base)
    base.attr_accessor :title
  end
end

class IncHookAttrUser
  include IncHookAttrs

  def go
    self.title = "x"
    title
  end
end
