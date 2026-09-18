# Scope-leak control (bead ita-s12, mutant b): one class declares TWO
# methods — the first sig declares `[T]`, the second sig has no type-param
# list at all and uses the real project class `T` nominally. Checking the
# first method's sig must never leave `T` bound to Unknown for the SECOND
# method's sig: the nominal call below must still accuse.
class T
end

class GenTMixed
  #: [T] (String, T) -> T
  def generic_method(name, value)
    value
  end

  #: (T) -> void
  def nominal_method(value)
  end
end

class GenTMixedCaller
  def call_it
    m = GenTMixed.new
    m.generic_method("k", [1, 2, 3])
    m.nominal_method(42)
  end
end
