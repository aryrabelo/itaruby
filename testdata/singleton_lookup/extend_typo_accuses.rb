# `extend` puts a module's INSTANCE methods on the class object, so
# `find_by_slug` is a real class method of Page and `find_by_slog` is a
# certain NoMethodError that itaruby does NOT report today — a known gap,
# characterized in crates/itaruby_semantic/tests/singleton_lookup.rs.
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
