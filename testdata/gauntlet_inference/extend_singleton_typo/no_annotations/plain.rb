# typed: true
# extend: the mixin lands on the SINGLETON, so `find_by_slug` is a class
# method of Page. The call misspells it — a real NoMethodError on the class.
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