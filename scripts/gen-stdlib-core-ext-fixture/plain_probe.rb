# Probe: an ordinary new method must still print exactly its own name
# (`Integer#plain_probe_name`), and nothing else.
class Integer
  def plain_probe_name = 1
end
