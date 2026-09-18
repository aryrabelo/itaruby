# Wave 14 control: undeclared siblings under the SAME namespaces wave 14
# curated members of. Each of these four ships in the same public gem as a
# name curated this round, and each sits one step away from a declared
# entry — but none was ever measured, so none is curated, and each must
# still warn E0104. This is the proof that wave 14's entries are exact-path
# declarations, not a `Karafka::`/`Flipper::`/`Money::`/`ActiveResource::`
# prefix door (the prefix-fallback bug, built and reverted 2026-08-26, must
# never come back).
#
# A control asserting a name is ABSENT has an expiry date: if a later wave
# curates any of these four, replace it here in the same commit.
class Wave14ControlProbe
  def call
    Karafka::Server.run
    Flipper::Adapters::Memory.new
    Money::Bank.instance
    ActiveResource::ResourceNotFound.new(nil)
  end
end
