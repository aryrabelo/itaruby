# Bead ita-s12: a generic-method type parameter declared by the sig's
# leading `[T]` list accepts ANY type for that parameter — `Array[Integer]`
# must not be rejected against `T`, the exact shape measured in ruby-lsp's
# `Document#cache_set` (`#: [T] (String, T) -> T`). The project ALSO
# declares a real class literally named `T` here, mirroring ruby-lsp's own
# vendored sorbet-runtime `T` module — the shape that made the OLD
# `RbsTy::Simple` fallback resolve `T` as that nominal class instead of the
# type variable it actually is inside this `[T]` sig.
class T
end

class GenTCache
  #: [T] (String, T) -> T
  def cache_set(request_name, value)
    @cache ||= {}
    @cache[request_name] = value
  end
end

class GenTCacheCaller
  def call_it
    cache = GenTCache.new
    cache.cache_set("textDocument/semanticHighlighting", [1, 2, 3])
  end
end
