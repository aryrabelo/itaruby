# frozen_string_literal: true

# Bead ita-asx, RESOLUTION side: a project reopening of `Class` defines an
# instance method of Class — every class object answers it (the exact
# shape activesupport's core_ext/class/subclasses.rb ships). The
# class-object track must consult the reopening: `Plant.descendants`
# resolves, MRI runs to completion.
class Class
  def descendants
    [42]
  end
end

class Plant; end

raise "bad" unless Plant.descendants == [42]
