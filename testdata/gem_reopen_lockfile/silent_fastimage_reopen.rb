# Bead ita-547: same override-table case as `wikicloth` — `fastimage` has
# no separator, so plain camelize alone would guess `Fastimage`, not the
# gem's real `FastImage` spelling. `fastimage` is declared in this
# directory's `Gemfile.lock`.
class FastImage
  def initialize(url)
    @url = url
  end
end

FastImage.new("https://example.com/image.png").width
