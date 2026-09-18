# typed: true
# DO NOT EDIT MANUALLY — toy fixture for bead ita-k9j.3 (RbiIndex union):
# a thin reopening of the SAME four classes `full.rbi` declares, exactly
# the `activestorage@*.rbi`/`flipper@*.rbi` shape measured against a
# real Tapioca corpus — plain `include` edges, none reaching
# `order_*_full_only_method`. Before this bead, `RbiIndex::constants`
# kept only ONE of `full.rbi`/`stub.rbi` (whichever `read_dir` happened
# to enumerate first) — if this stub file won the race, the method BFS
# never saw `full.rbi`'s deeper edges at all, only these dead-end stub
# mixins.
#
# `RbiUnionOrderD::Base`'s second edge is `extend`, not `include`
# (`tests/rbi_union.rs::union_keeps_each_files_methods_on_its_own_track`):
# `StubMixinTwo`'s instance method lands on `RbiUnionOrderD::Base`'s
# SINGLETON track (rule 2, `index.rs::compute_rbi_method_closure`'s doc
# comment), while `full.rbi`'s `order_d_full_only_method` stays on the
# INSTANCE track — a union that accidentally blended the two files'
# tracks together (instead of keeping each file's own edges on the
# track that reached IT) would leak one onto the other's side.

class RbiUnionOrderA::Base
  include RbiUnionOrderA::StubMixinOne
  include RbiUnionOrderA::StubMixinTwo
end

class RbiUnionOrderB::Base
  include RbiUnionOrderB::StubMixinOne
  include RbiUnionOrderB::StubMixinTwo
end

class RbiUnionOrderC::Base
  include RbiUnionOrderC::StubMixinOne
  include RbiUnionOrderC::StubMixinTwo
end

class RbiUnionOrderD::Base
  include RbiUnionOrderD::StubMixinOne
  extend RbiUnionOrderD::StubMixinTwo
end

module RbiUnionOrderD::StubMixinTwo
  sig { returns(Integer) }
  def order_d_stub_singleton_only_method; end
end
