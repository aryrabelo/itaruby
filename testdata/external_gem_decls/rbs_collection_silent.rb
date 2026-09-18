# Bead ita-dpg.1 mutation probe (PACK leaf): `Faker::Config`,
# `Nokogiri::XML`, and `Stripe::Charge` are three entries from DIFFERENT
# gem families inside the generated
# `crates/itaruby_semantic/declarations/rbs_collection.rbi`
# (`scripts/gen-rbs-collection-pack.rb`) — before this bead all three were
# unresolved constants (E0104 x3); after, each resolves and stays open
# (an unknown method call on any of them never becomes a false E0101, same
# suppression-only contract as `declarations/gems.rbi`), so this whole
# file is silent.
#
# MUTANT THIS FILE MUST CATCH: removing any ONE of the three corresponding
# lines from `rbs_collection.rbi` re-emits E0104 for exactly that one
# constant — proven by `tests/declarations.rs`'s
# `pack_silences_constants_across_different_gem_families` test asserting
# `diags.is_empty()`.
class RbsPackSilentProbe
  def call
    Faker::Config.locale
    Nokogiri::XML.parse("<a/>")
    Stripe::Charge.retrieve("ch_123")
  end
end
