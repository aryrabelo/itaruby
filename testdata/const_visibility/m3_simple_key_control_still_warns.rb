# Bead ita-9he M3 control: the read-side fallback for an unresolved owner
# must key `toplevel_consts` by the FULL written path, never by the bare
# simple segment. Here a top-level `ConstVisM3Inner` write exists, so a
# mutant that keys the fallback by `simple` would wrongly resolve
# `ConstVisMissing::ConstVisM3Inner` to it and go silent. The owner
# `ConstVisMissing` resolves nowhere, and no `ConstVisMissing::ConstVisM3Inner`
# write exists either, so E0104 MUST fire.
#
# Mutants: (a) `if prefix.is_empty() { simple } else { name }` → `simple`
# (always simple) → this fixture goes silent (was BLIND before this control
# existed); (b) correct code → exactly one E0104 below.
ConstVisM3Inner = 1

class ConstVisM3Reader
  def read
    ConstVisMissing::ConstVisM3Inner
  end
end
