# Bead ita-d2: `SystemStackError` is a genuine core exception class
# (`ruby --disable-gems -e 'p defined?(SystemStackError)'` => "constant")
# that `core_inventory.txt` never harvested before this bead — the
# generator only walked a fixed list of non-exception classes for
# METHODS, never `Object.constants` for constant names at all. A bare
# reference here must resolve silently: no E0104.
class CoreConstsRescuer
  def recurse
    recurse
  rescue SystemStackError => e
    e.message
  end
end
