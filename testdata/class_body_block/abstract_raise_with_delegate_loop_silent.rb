# The discriminating shape (discourse script/import_scripts/base.rb
# reality): the SAME class carries BOTH the abstract-raise stub FIRST
# (there :123) and the delegate loop AFTER (there :141). First-reason-wins
# recorded AbstractRaise alone, and the instance-lookup softening then
# treated the class as a pure abstract stub, firing false E0101s on every
# delegated name. The precedence rule (AbstractRaise is the WEAKEST open
# reason) makes the class ClassBodyBlock-open instead. Expected: ZERO
# diagnostics.
class AbstractRaiseWithDelegateLoop
  def render
    raise NotImplementedError, "subclass"
  end

  %i[foo bar].each { |m| delegate m, to: :@lookup }

  def draw
    foo
  end
end

class AbstractRaiseWithDelegateLoopKid < AbstractRaiseWithDelegateLoop
end

AbstractRaiseWithDelegateLoopKid.new.draw
