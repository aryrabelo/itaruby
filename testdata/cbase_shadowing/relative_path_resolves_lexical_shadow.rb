# Control for ita-yh1: the SAME shadowing shape as
# `cbase_ignores_lexical_shadow.rb`, but WITHOUT the leading `::`. Real
# Ruby constant lookup for a bare/relative path starts from lexical
# nesting, so `CbaseShTarget3` here correctly resolves to the nested
# `CbaseShHost3::CbaseShTarget3` shadow, NOT the unrelated top-level module
# of the same name (they are not related by inheritance, so the top-level
# module's constants are never visible through the shadow). That shadow
# has no `VALUE` member, so genuine Ruby raises `NameError` here — this
# fixture pins that CURRENT (and correct) itaruby behavior; it is not a
# regression target for ita-yh1's cbase fix, which only changes resolution
# for paths that actually start with `::`.
module CbaseShTarget3
  VALUE = "top-level"
end

module CbaseShHost3
  module CbaseShTarget3
  end

  class Consumer
    X = CbaseShTarget3::VALUE
  end
end
