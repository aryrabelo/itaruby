# `rspec` in the lock, `RSpec` in the code: the letters agree, the case
# does not. Before `gem_namespace_key`, the camelize guess `Rspec` missed
# this by exact-equality, so this reopening of the gem's own namespace
# looked like a complete project definition and every method the gem
# really provides became a candidate accusation.
module RSpec
  module Core
    class ExampleGroup
      def run_twice
        # `metadata` is the gem's, not this file's.
        [metadata, metadata]
      end
    end
  end
end

RSpec::Core::ExampleGroup.new.run_twice
