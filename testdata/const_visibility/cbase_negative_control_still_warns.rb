# Anti-suppression control (bead ita-9he): a cbase reference (`::CONST`) to
# a constant that is never written anywhere must still warn E0104.
# Exercises the SAME read-side branch (`const_exists`'s empty-`prefix`
# fallback) the cbase-write fix touches -- if that fallback were ever
# changed to return `true` unconditionally, or to check the wrong
# `toplevel_consts` key, this genuinely undefined constant would go
# silent. Must STILL WARN E0104.
class ConstVisCbaseNegative
  def missing
    ::ConstVisCbaseNeverDefinedAnywhere
  end
end
