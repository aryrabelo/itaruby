# Bead ita-dpg.1: same shape as `probe.rb`, except the gem-side
# superclass is a namespace one of this repo's own curated declarations
# files NAMES (`declarations/rbs_collection.rbi` declares
# `GraphQL::Schema::Object`), so it RESOLVES in the project index as a
# force-open `OpenReason::DeclaredExternal` class instead of failing to
# resolve at all. `SYNTHIDENT` still lives only on the mixin the RBI puts
# in that superclass's chain, so the RBI walk must still start from the
# declared ancestor — `unresolved_ancestors` alone never sees it, and
# `external_lookup_starts` consulting only that population is what made
# 2567 corpus-c warnings reappear the day the pack landed.
class RbiAncDeclaredSuperclass < GraphQL::Schema::Object
  def m(x)
    SYNTHIDENT
  end
end
