# Bead ita-vto: `TapiocaVtoFixtureGem::Base` (declared in the toy RBI next
# to this file) resolves the constant but must never close ancestry for
# method lookup — same invariant #1 contract `testdata/external_gem_decls/
# model.rb` proves for the curated `gems.rbi`. `undefined_method` has no
# definition anywhere in this class and none declared on
# `TapiocaVtoFixtureGem::Base` either (the RBI declares an empty class,
# zero methods, by design — see `merge_declared_fragment`'s doc comment):
# it must stay silent, never a false E0101.
class TapiocaVtoSubclass < TapiocaVtoFixtureGem::Base
  def call
    undefined_method
  end
end
