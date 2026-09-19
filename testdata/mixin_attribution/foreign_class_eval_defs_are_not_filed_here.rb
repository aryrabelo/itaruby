# `Other.class_eval <<-RUBY` runs the string in OTHER, so the names it
# defines belong to Other. The enclosing class gains nothing from it, and
# the harvest must not file them here (measured 2026-09-19: it filed on
# the enclosing fragment regardless of the eval call's receiver, which
# invented a method this class never gains and fed
# `dynamic_mixin_covers` a name this class's mixers do not have).
#
# This fixture is about the FILING, not about a diagnostic: the enclosing
# class opens anyway for the `class_eval` call itself
# (`OpenReason::EvalOrSend`), so the diagnostic side cannot tell the two
# behaviours apart — the index-level test does
# (`foreign_class_eval_defs_are_not_filed_here`). The name is dropped
# rather than filed on the class it really belongs to: filing onto a
# foreign fragment is a separate decision (`apply_body_def` documents the
# declared-owner rule it needs), so that stays roadmap, fail-closed.
class MixAttrForeignEvalOther
end

class MixAttrForeignEvalBar
  %w(mix_attr_foreign_eval_absent).each do |method|
    MixAttrForeignEvalOther.class_eval <<-RUBY, __FILE__, __LINE__ + 1
      def #{method}
        "defined on Other"
      end
    RUBY
  end

  def rakefile
    mix_attr_foreign_eval_absent
  end
end

MixAttrForeignEvalBar.new.rakefile
