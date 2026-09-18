# Control fixture for bead ita-dpg.1 (mirrors `still_unresolved.rb` for
# bead ita-3gs): `Prism`, `Fabrication`, and `Selenium` are genuinely NOT
# declared anywhere in `declarations/gems.rbi` or the generated
# `declarations/rbs_collection.rbi` — none of the pack's frozen gem list
# (`scripts/gen-rbs-collection-pack.rb`'s `GEMS` list) ships one of those
# namespaces, and none of the listed gems' own `.rbs` files reopen one
# either. Each reference below must still warn E0104 — the pack is an
# allowlist for a fixed, measured gem set, never a blanket "assume every
# unresolved top-level constant is some gem".
#
# This fixture's third example has now been replaced TWICE by its own
# subject matter, which is the point worth remembering: bead ita-dpg.2
# swapped `Faraday` (its `faraday` gem joined the frozen list, 31 measured
# sites) for `RSpec`, and the wave-10 integration swapped `RSpec` in turn,
# because bead ita-dpg.3 landed `RSpec` in `gems.rbi` (685 measured sites)
# in a sibling slice cut from the same commit. A control keyed on a name
# being ABSENT decays every time the allowlist grows — pick a name whose
# absence was just re-checked mechanically against both declaration files,
# and expect to re-check it again. `Fabrication` is the fabrication gem's
# own module name, distinct from the `Fabricate` class `gems.rbi` declares.
class RbsPackUndeclaredProbe
  def call
    Prism::ProgramNode.new
    Fabrication::Config.new
    Selenium::WebDriver.for(:chrome)
  end
end
