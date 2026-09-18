# Bead ita-c8h (Round-5 audit, 7 sites mastodon): mirrors the exact
# `Rack::Attack::Request` shape found in `config/initializers/
# rack_attack.rb` — the project reopens a class nested under a namespace
# (`GemSuperReopenGemNs`) it never itself wraps with a bare `class`/
# `module` anywhere in this fixture. That namespace's real definition
# lives only inside an external gem this checker never modeled (the real
# `Rack::Attack::Request < Rack::Request`, whose superclass carries
# `ip`/`path`/`params`), so a call to a method this file never defines
# must stay silent — the same reasoning `apply_gem_reopenings` (ita-547)
# already applies from a `Gemfile.lock` gem-name guess, generalized here
# (`apply_undeclared_namespace_reopenings`) to work with no
# `Gemfile.lock` at all: this fixture has none.
class GemSuperReopenGemNs::Request
  def local_helper
    1
  end
end

GemSuperReopenGemNs::Request.new.gem_only_inherited_method
