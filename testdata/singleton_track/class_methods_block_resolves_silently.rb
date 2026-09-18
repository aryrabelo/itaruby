# `class_methods do ... end` — ActiveSupport::Concern's block spelling of
# the `ClassMethods` module. The miniature below is faithful enough that
# MRI runs this file to completion.
module ActiveSupport
  module Concern
    def class_methods(&block)
      mod = const_defined?(:ClassMethods) ? const_get(:ClassMethods) : const_set(:ClassMethods, Module.new)
      mod.module_eval(&block)
    end

    def append_features(base)
      super
      base.extend const_get(:ClassMethods) if const_defined?(:ClassMethods)
    end
  end
end

module Countable
  extend ActiveSupport::Concern

  class_methods do
    def count_for(scope)
      "count:#{scope}"
    end
  end
end

class Report
  include Countable
end

raise "block form" unless Report.count_for("all") == "count:all"
