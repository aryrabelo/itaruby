# The silence side of the same shape: the name IS supplied by the extended
# module, so the lookup resolves and nothing is reported.
module Findable
  def find_by_slug(slug)
    new(slug)
  end
end

class Page
  extend Findable

  def initialize(slug)
    @slug = slug
  end
end

Page.find_by_slug("home")
