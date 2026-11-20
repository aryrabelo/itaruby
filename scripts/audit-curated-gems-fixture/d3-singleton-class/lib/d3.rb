# Item 3: defs inside `class << self` are singleton methods, never
# instance methods of the enclosing core class (or of `Object`).
class String
  class << self
    def d3_build = new
    include Comparable
  end
end

class << self
  def d3_helper = 1
end
