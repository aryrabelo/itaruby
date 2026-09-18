# Precedence pin: a sig with NO leading `[T, ...]` list that happens to
# mention a project class literally named `T` must resolve `T` as that
# nominal class exactly as before — the type-param binding only applies
# INSIDE a sig that actually declares `[T]`.
class T
end

class GenTNominalHolder
  #: (T) -> void
  def use(value)
  end
end

class GenTNominalCaller
  def call_it
    GenTNominalHolder.new.use(42)
  end
end
