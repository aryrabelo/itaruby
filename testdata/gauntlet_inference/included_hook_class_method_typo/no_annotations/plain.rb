# typed: true
# The `self.included` + `base.extend(ClassMethods)` idiom, the backbone of
# every Rails concern: `default_scope_name` really is a class method of
# Record, and the call misspells it.
module Scopeable
  def self.included(base)
    base.extend(ClassMethods)
  end

  module ClassMethods
    def default_scope_name
      "all"
    end
  end
end

class Record
  include Scopeable
end

Record.default_scope_nmae