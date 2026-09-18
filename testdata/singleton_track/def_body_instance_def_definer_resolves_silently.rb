# frozen_string_literal: true

# In an INSTANCE method body `self` is an instance, not the class:
# `define_singleton_method` there lands on that ONE object, so the names
# are not the class's and the attribution is not provable. The class
# fails closed — open, not enriched — and the call on the object that
# really got the method stays silent. MRI runs this file to completion
# (and `Widget.new.zoom` on a DIFFERENT instance would be a
# NoMethodError, which is exactly why the names are not filed).
class Widget
  def install
    define_singleton_method(:zoom) { 2 }
    self
  end
end

w = Widget.new.install
raise 'zoom' unless w.zoom == 2
