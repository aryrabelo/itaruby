# The correct spelling resolves: no record at all (Found never enters the
# census; silence stays silence).
module Findable
  def find_by_slug(slug)
    new(slug)
  end
end

class Page
  extend Findable
end

Page.find_by_slug("home")
