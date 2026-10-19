# frozen_string_literal: true

# 2026-10-19, CONTROL for the shape rails ships
# (`class Object; include ActiveSupport::Tryable`): the mixin is a module
# the index holds OPEN (a computed `define_method`), and it is included by
# a reopening of `Object`, which sits on every class object's dispatch.
# Its LITERAL definition still resolves (`mrm_literal` - the brew shape,
# where the declared-open `T::Sig` literally defines `sig` in the vendored
# sorbet-runtime). Its openness must NOT make every class object
# unreadable: a name nothing defines stays a conclusive E0101, exactly as
# it was before the consult read mixins at all.
module MrmOpaque
  %w[mrm_computed].each do |n|
    define_method(n) { :computed }
  end

  def mrm_literal
    :literal
  end
end

class Object
  include MrmOpaque
end

class MrmPier; end

raise "bad" unless MrmPier.mrm_literal == :literal
MrmPier.mrm_nothing
