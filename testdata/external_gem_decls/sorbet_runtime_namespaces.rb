# Sorbet-runtime's own T::* namespaces beyond the core five — every read
# below was a measured tapioca E0104 before the 2026-08-26 entries
# (T::Module x4, T::Props::ClassMethods x2, T::Private::Types::Void x2,
# T::Private::Abstract::Data x2, one each of the rest). All must stay
# silent now; classes are prefixed TSRuby* so `ita check testdata/` never
# merges them with a same-named fixture elsewhere.
class TSRubyGeneric
  #: (TSRubyGeneric) -> void
  def wrap(other)
    T::Module
    T::Props::ClassMethods
    T::Private::Types::Void
    T::Private::Abstract::Data
    T::Types::Proc
    T::Types::Base
    T::Sig::WithoutRuntime
    T::Set
    T::Enumerable
    T::Private::Methods::DeclBuilder
  end
end
