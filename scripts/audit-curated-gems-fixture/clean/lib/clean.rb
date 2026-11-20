# Silence side: nothing here touches a core-chain class or Object.
module Clean
  class Thing
    include Enumerable
    alias each_thing each
    def each = yield(1)

    class << self
      def build = new
      include Comparable
    end
  end
  Thing.instance_exec { 1 }
end
