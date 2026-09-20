# Gap 2: the concern backbone. `default_scope_name` really is a class method
# of Record; the misspelling raises. Same census expectation as extend_typo.
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
