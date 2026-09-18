# Same shape; the class method takes exactly one argument, so MRI raises
# ArgumentError (`given 0, expected 1`) on the last line.
module ActiveSupport
  module Concern
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

Dashboard.fetch_cached_stats
