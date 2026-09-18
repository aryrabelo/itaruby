# Bead ita-vto mutation probe: `TapiocaVtoFixtureGem::Widget` only exists in
# the toy `sorbet/rbi/gems/tapioca_vto_fixture_gem.rbi` fixture next to this
# file, never in project code. Before this bead's RBI loading, a project
# with no RBI support at all resolves nothing here — `TapiocaVtoConsumer`
# below stays unresolved (E0104), exactly like `testdata/external_gem_decls/
# still_unresolved.rb` proves for a name outside the curated list. After
# this bead: `sorbet/rbi` is discovered from this fixture's own root,
# `TapiocaVtoFixtureGem::Widget` resolves the constant, and this file is
# silent.
class TapiocaVtoConsumer
  def build
    TapiocaVtoFixtureGem::Widget.new
  end
end
