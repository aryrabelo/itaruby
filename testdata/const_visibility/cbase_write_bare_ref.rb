# Bead ita-9he: a plain class method reading the top-level `ConstVisCbaseDb`
# (defined via cbase write in cbase_write_def.rb, same project). Must
# become silent.
class ConstVisCbaseReader
  def read
    ConstVisCbaseDb
  end
end
