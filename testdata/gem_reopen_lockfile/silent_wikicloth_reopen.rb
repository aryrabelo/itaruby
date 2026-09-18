# Bead ita-547: `wikicloth` has no internal `_`/`-` separator, so the plain
# camelize heuristic alone would guess `Wikicloth`, not the gem's real
# `WikiCloth` spelling — this fixture exercises the small override table
# in `discovery.rs`'s `gem_namespace`. `wikicloth` is declared in this
# directory's `Gemfile.lock`.
class WikiCloth
  def initialize(text)
    @text = text
  end
end

WikiCloth.new("wiki text").to_html
