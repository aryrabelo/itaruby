# Bead ita-o8l.1: proves the method-name-keyed dynamic-mixin softening
# that replaces bead ita-a8z's rejected `Global`/`BuilderName`
# candidates. `Ito8lGeneratorsSetup.builder` mirrors the real false
# positive this bead was filed against — `railties/lib/rails/generators/
# app_base.rb:167-172`:
#
#   def builder
#     @builder ||= begin
#       builder_class = get_builder_class
#       builder_class.include(ActionMethods)
#       builder_class.new(self)
#     end
#   end
#
# a plain class gets a real module mixed in through a call whose
# RECEIVER is a local variable (never modeled statically) but whose
# ARGUMENT is a literal constant. `Ito8lActionMethods` here plays
# `Rails::ActionMethods`; `Ito8lAppBuilder` plays `Rails::AppBuilder`.
#
# All three classes below share this ONE file/project on purpose: the
# controls only prove anything if the dynamic-mixin machinery above them
# is genuinely active in the SAME project, not merely absent.

module Ito8lActionMethods
  def ito8l_forwarded_method
    "forwarded"
  end
end

module Ito8lGeneratorsSetup
  def self.builder
    builder_class = Ito8lAppBuilder
    builder_class.include(Ito8lActionMethods)
    builder_class
  end
end

# SILENT: `ito8l_forwarded_method` is defined ONLY on
# `Ito8lActionMethods`, mixed in above through a receiver
# (`builder_class`) this checker can never resolve statically.
# `Ito8lAppBuilder` itself never includes anything — its own ancestry
# looks fully closed — so without this bead's fix this call is a
# fabricated E0101.
class Ito8lAppBuilder
  def run
    ito8l_forwarded_method
  end
end

# FIRES (control): a DIFFERENT class in the SAME project as the dynamic
# mixin above, calling a method NO dynamically-mixed module defines.
# Proves the softening is keyed on the METHOD NAME, never on "this
# project happens to have a dynamic mixin somewhere" — a blanket
# suppressor (bead ita-a8z's rejected `Global` candidate) would go
# silent here too.
class Ito8lUnrelatedCaller
  def run
    ito8l_never_provided_by_any_mixin
  end
end

# FIRES (control, re-measures the ita-a8z `BuilderName` collateral false
# negative): bead ita-a8z's rejected `BuilderName` candidate opened
# every class whose name ends in `Builder`, purely by NAME, regardless
# of whether that class was anywhere near the dynamic-receiver call —
# measured to silence a real, unrelated E0101 three files away in that
# candidate's own corpus run. This bead's fix never inspects the
# receiving class's name at all (see `MixinTargetScan`'s doc comment in
# `index.rs`), so an innocent `*Builder`-named class with a genuinely
# undefined method must still warn.
class Ito8lWidgetBuilder
  def run
    ito8l_widget_missing_method
  end
end
