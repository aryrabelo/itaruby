# Gap 1: `extend Findable` splices find_by_slug onto Page's singleton; the
# call misspells it. MRI raises NoMethodError. The census must bucket this
# site closed_notfound — it is exactly what a class-object E0101 would fire
# on once the track arms.
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

Page.find_by_slog("home")
