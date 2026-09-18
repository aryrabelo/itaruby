# Bead ita-54k G2: an alias to a name the project never defines
# anywhere is a real Ruby `NameError` at the alias site itself. The
# chase must degrade to a silent miss (no crash) and must NOT fabricate
# a suppression: `ConstAliasToUndefined::WHATEVER` keeps warning E0104.
ConstAliasToUndefined = ConstAliasNeverDefinedAnywhere

class ConstAliasUndefinedReader
  def read
    ConstAliasToUndefined::WHATEVER
  end
end
