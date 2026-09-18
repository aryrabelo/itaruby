# Bead ita-6bq: the REAL rails/activesupport `DeprecationProxy` shape.
# The parent defines a variadic `self.new` AND opens itself via a
# class-body block (`instance_methods.each { |m| undef_method m ... }` —
# ita-d0j: any class-body call with a block, `define_method` aside, opens
# the class because it may define/undefine anything). That means
# `lookup_singleton(child, "new")` returns `Inconclusive`, NOT `Found` —
# the walk hits the OPEN parent before ever reaching a definitive answer.
# Falling through to the `initialize`-based match here would be unsound:
# an invisible ancestor `self.new` may still govern arity, so the child's
# OWN `initialize(a, b, c)` (arity 3) proves nothing about what
# `.new(...)` really accepts. The 2-argument call must stay silent.
class NewSelfOvOpenParent
  def self.new(*args, **kwargs, &block)
    allocate
  end

  instance_methods.each { |m| undef_method m unless /^__/.match?(m) }
end

class NewSelfOvOpenChild < NewSelfOvOpenParent
  def initialize(object, message, deprecator)
  end
end

NewSelfOvOpenChild.new(nil, "message")
