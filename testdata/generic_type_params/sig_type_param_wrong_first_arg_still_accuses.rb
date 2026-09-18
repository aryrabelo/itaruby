# Control: the SAME `[T]` sig as sig_type_param_accepts_any_arg_silent.rb,
# but the mismatch is on the sig's own NON-generic `String` parameter
# (position 1), not on `T` — binding `T` to Unknown must never widen an
# unrelated, concretely-typed parameter's check.
class GenTCache2
  #: [T] (String, T) -> T
  def cache_set(request_name, value)
    @cache ||= {}
    @cache[request_name] = value
  end
end

class GenTCache2Caller
  def call_it
    cache = GenTCache2.new
    cache.cache_set(1, "value")
  end
end
