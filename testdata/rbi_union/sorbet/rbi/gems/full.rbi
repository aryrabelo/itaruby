# typed: true
# DO NOT EDIT MANUALLY — toy fixture for bead ita-k9j.3 (RbiIndex union),
# mirroring the real corpus shape this bead was measured against: a full
# gem declaration (`sorbet/rbi/gems/activerecord@*.rbi`, 123 direct
# edges) reopened elsewhere by thin stub gems (`activestorage@*.rbi`, 8
# edges; `flipper@*.rbi`, 5 edges). Two `include` hops reach a module
# that declares a method `stub.rbi` never mentions — the method only
# resolves when `build_rbi_index` unions THIS file with the stub, no
# matter which one `discover_rbi_files` happens to enumerate first.
#
# Four independent constant names, one per concern `tests/rbi_union.rs`
# proves, all carrying the same full-declaration shape — every test in
# that file uses its OWN name because `rbi_method_closure`'s BFS memo is
# keyed on the start name alone, process-wide, shared by every test in
# this binary (see `tests/rbi_methods.rs`'s header comment): a shared
# name across two assertions that expect different discovery orders
# would just replay the first order's cached result.
#
#   RbiUnionOrderA — discovery order [full, stub]
#   RbiUnionOrderB — discovery order [stub, full] (reversed)
#   RbiUnionOrderC — the undeclared-method / never-manufactures-a-
#     diagnostic proof (order irrelevant here)
#   RbiUnionOrderD — the track-separation proof (`stub.rbi` only)

class RbiUnionOrderA::Base
  include RbiUnionOrderA::Helpers
end

module RbiUnionOrderA::Helpers
  include RbiUnionOrderA::Deep
end

module RbiUnionOrderA::Deep
  sig { returns(String) }
  def order_a_full_only_method; end
end

class RbiUnionOrderB::Base
  include RbiUnionOrderB::Helpers
end

module RbiUnionOrderB::Helpers
  include RbiUnionOrderB::Deep
end

module RbiUnionOrderB::Deep
  sig { returns(String) }
  def order_b_full_only_method; end
end

class RbiUnionOrderC::Base
  include RbiUnionOrderC::Helpers
end

module RbiUnionOrderC::Helpers
  include RbiUnionOrderC::Deep
end

module RbiUnionOrderC::Deep
  sig { returns(String) }
  def order_c_full_only_method; end
end

class RbiUnionOrderD::Base
  include RbiUnionOrderD::Helpers
end

module RbiUnionOrderD::Helpers
  include RbiUnionOrderD::Deep
end

module RbiUnionOrderD::Deep
  sig { returns(String) }
  def order_d_full_only_method; end
end
