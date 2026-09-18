# `ActiveSupport::Concern` + a nested `ClassMethods` module: the gem's own
# `append_features` extends `ClassMethods` onto every includer's singleton.
# A faithful miniature of the gem stands in for it, so MRI really runs
# this file to completion.
module ActiveSupport
  module Concern
    def self.extended(base)
      base.instance_variable_set(:@_dependencies, [])
    end

    def append_features(base)
      super
      base.extend const_get(:ClassMethods) if const_defined?(:ClassMethods)
    end
  end
end

module StatsCacheable
  extend ActiveSupport::Concern

  module ClassMethods
    def fetch_cached_stats(key)
      "stats for #{key}"
    end
  end
end

class Dashboard
  include StatsCacheable
end

raise "concern" unless Dashboard.fetch_cached_stats("x") == "stats for x"
