# Bead ita-47y: must complete (not hang) and keep warning E0104 — a
# cycle is a genuine miss, never a suppression.
class ConstAlias47yCycleReader
  def read
    ConstAlias47yCycleA::Deep::NOPE
  end
end
