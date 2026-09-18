# Bead ita-083 mutation probe (same category as ita-3gs's model.rb).
# `T::Struct` and `TracePoint` are curated external declarations
# (crates/itaruby_semantic/declarations/gems.rbi): before this bead both
# were unresolved constants (E0104 x2, measured in the ita-40k head-to-head
# against ruby-lsp/tapioca, both `typed: strict`); after, they resolve and
# this whole file is silent.
#
# `undefined_field` below has no local definition anywhere in this class
# and no known one on `T::Struct` either — it must stay silent rather than
# become a false E0101, because the declaration never closes ancestry
# (invariant #1): we don't know if the real `T::Struct` defines it via
# `prop`/`const`-generated accessors or metaprogramming.
class StrictCoreDeclConfig < T::Struct
  def log_and_probe
    tp = TracePoint.new(:call) { |t| t.method_id }
    tp.enable
    undefined_field
  end
end
