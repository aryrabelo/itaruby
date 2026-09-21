# frozen_string_literal: true

# Bead ita-asx, ACCUSATION side: the reopening carries a REAL signature —
# `descendants(need)` takes one argument, so the zero-argument call on the
# class object is an E0102 MRI really raises (ArgumentError, given 0
# expected 1). Resolution is not just silence; it is checkable knowledge.
class Class
  def descendants(need)
    need
  end
end

class Plant; end

Plant.descendants
