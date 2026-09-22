# Bead ita-esc, fail-closed side: the hook's `base` escapes into a nested
# block the shallow harvest never reads, so what it installs cannot be
# enumerated and the extender's surface is UNKNOWN, not empty. discourse's
# `Migrations::Enum` installs `valid?`/`values` inside
# `TracePoint.new(:end) do |tp| ... end.enable`, and four census residue
# records read as conclusive misses on them before this.
module EscEnum
  def self.extended(base)
    [1].each do
      base.define_singleton_method(:esc_values) { [3] }
    end
  end
end

module EscHashtagType
  extend EscEnum
end

raise 'the hook must really install it' unless EscHashtagType.esc_values == [3]
