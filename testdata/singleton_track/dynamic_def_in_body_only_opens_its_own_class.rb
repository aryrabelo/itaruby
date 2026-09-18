# The attribution rule: `Inner.class_eval { define_method(name) ... }`
# inside `def self.patch` defines on Inner, and says nothing about the
# enclosing class - which therefore stays CLOSED and keeps being checked.
# Measured on rails: attributing that nested call to the enclosing class
# opened ActionDispatch::Routing::RouteSet and swallowed a baseline
# E0101.
class Outer
  class Inner
  end

  def self.one(a)
    a
  end

  def self.patch(name)
    Inner.class_eval { define_method(name) { 1 } }
  end
end

Outer.one(1, 2)
