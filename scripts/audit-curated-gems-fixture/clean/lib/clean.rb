# Silence side: nothing here touches a core-chain class or Object. The
# file loads clean under MRI (`def each` precedes its alias).
module Clean
  class Thing
    include Enumerable
    def each = yield(1)
    alias each_thing each

    class << self
      def build = new
      include Comparable
    end
  end
  Thing.instance_exec { 1 }
  Thing.extend(Comparable)
end
