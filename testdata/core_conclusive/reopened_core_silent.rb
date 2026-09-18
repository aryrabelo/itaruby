# ita-2ve closed-world, SILENT: the project reopens Array with its own
# method, so by_path["Array"] exists and Array's conclusive lookup stands
# down (condition (b)). The method used here is real project code, not a
# core method -> silence.
#
# Why Array and not String (the spec's example): `ita check testdata/`
# runs the whole tree as ONE merged project, and reopening String here
# would globally silence every String-typo fixture too (narrowing_core_typo,
# ivar_core_typo, with_gemfile/). The by_path mechanism proven is
# identical; the String variant is proven in isolation by the unit tests
# in crates/itaruby_semantic/tests/core_conclusive.rs.
#
# The reopened method's body is deliberately literal-only: `self` inside
# a reopened core class types as a PROJECT instance (Instance(Array-id)),
# so calling a real core method like `first` on it would take the
# project-class lookup path and E0101 — a separate v0 modeling gap this
# bead does not touch.
class Array
  def core_conclusive_reopened_item
    42
  end
end

class CoreConclusiveReopener
  def use_it
    [1, 2].core_conclusive_reopened_item
  end
end
