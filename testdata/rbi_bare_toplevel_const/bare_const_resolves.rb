# Bead ita-h5s: a bare, unqualified top-level reference to `XClient`
# must resolve silently through the vendored RBI's bare value-constant
# write (`XClient = T.let(...)` at `sorbet/rbi/shims/x_client.rbi`'s true
# file scope). `undeclared` names a sibling constant that is declared
# NOWHERE (not the project, not the RBI) — anti-suppression control, so
# this channel cannot become a blanket silencer for every bare constant.
class Rbih5sReader
  def read
    XClient
  end

  def undeclared
    UndeclaredConst
  end
end
