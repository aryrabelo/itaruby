# The Rails concern backbone: `def self.included(base); base.extend(
# ClassMethods); end`. `default_scope_name` really becomes a class method of
# Record, so the misspelling is a certain NoMethodError — one itaruby does
# NOT report today, a known gap characterized in
# crates/itaruby_semantic/tests/singleton_lookup.rs.
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
