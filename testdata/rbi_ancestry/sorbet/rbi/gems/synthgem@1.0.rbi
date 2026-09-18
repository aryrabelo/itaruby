# Synthetic Tapioca-style RBI (W3 external-ancestry fixture). Shapes are
# exactly what Tapioca emits for a public gem: qualified class headers,
# `include` edges, and toplevel qualified value-constant writes. The names
# are synthetic (`SynthGem::*`), not any app or real-gem symbol.
class SynthGem::Schema::Object < ::SynthGem::Schema::Member; end
class SynthGem::Schema::Member
  include ::SynthGem::TypeNames
end
module SynthGem::TypeNames; end
SynthGem::TypeNames::IDENT = T.let(T.unsafe(nil), String)

# Bead ita-dpg.1: the same gem-side shape, but hung off a namespace this
# repo's own `declarations/rbs_collection.rbi` declares — so the project
# index RESOLVES the superclass (force-open `DeclaredExternal`) and the
# RBI walk has to start from a declared ancestor, not an unresolved name.
class GraphQL::Schema::Object < ::GraphQL::Schema::Member; end
class GraphQL::Schema::Member
  include ::GraphQL::SynthTypeNames
end
module GraphQL::SynthTypeNames; end
GraphQL::SynthTypeNames::SYNTHIDENT = T.let(T.unsafe(nil), String)
