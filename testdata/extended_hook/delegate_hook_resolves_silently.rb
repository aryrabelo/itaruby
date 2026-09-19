# Bead H, onda 2 — the ActiveModel::Naming shape.
#
# `Naming.self.extended(base)` calls `base.delegate :model_name, to:
# :class` (`activemodel/lib/active_model/naming.rb:263-266`), and
# `delegate` installs `model_name` as an INSTANCE method of the extending
# class. `Blog::Post` does `extend ActiveModel::Naming`
# (`activemodel/test/models/blog_post.rb:9`), so `Blog::Post.new.
# model_name` — code that runs — was one of rails' baseline errors
# (`activemodel/test/cases/naming_test.rb:333`).
#
# `extend M` alone reads M's own instance methods onto the extender's
# SINGLETON track; nothing about the install this hook performs on the
# extender's INSTANCE surface is in any fragment.
#
# MRI: prints "ExtHookNamingPost" twice — once with no argument, once with
# one, because a delegated method takes whatever its target takes (which is
# why the checker must not model its arity) — then raises NoMethodError on
# `never_installed`, a name the hook never installs.
# `ita check`: exactly ONE error, on `never_installed`. The install is
# ENUMERATED, never a blanket open: a hook naming its methods does not make
# the rest of the surface unknowable.
class Module
  # A stand-in for ActiveModel's `Module#delegate`, so this fixture really
  # runs under MRI. The checker never reads this body: the call site it
  # models is `base.delegate :model_name, to: :class` in the hook below.
  def delegate(*names, to:, **)
    names.each { |n| define_method(n) { |*| public_send(to) } }
  end
end

module ExtHookNaming
  def self.extended(base)
    base.delegate :model_name, to: :class
  end
end

class ExtHookNamingPost
  extend ExtHookNaming
end

puts ExtHookNamingPost.new.model_name
puts ExtHookNamingPost.new.model_name(:anything)

ExtHookNamingPost.new.never_installed