# Bead ita-zdy control: a declared union with an UNRESOLVABLE member
# (`UnionElimUnknownExternalGemClass` is never defined anywhere in
# `testdata/`) never actually becomes a `Ty::Union` in the first place —
# `Ty::union` collapses straight to `Ty::Unknown` the moment any part is
# `Unknown` (see `types.rs`), so `x`'s declared type here is plain
# `Ty::Unknown`, not a union. `eliminate_union_member`'s guard against
# `Unknown` members is proven directly (hand-constructed) in
# `check.rs`'s `union_elimination_tests` module; this fixture proves the
# END-TO-END consequence: a real project-class-plus-unresolvable-member
# signature must never manufacture a diagnostic in the false branch,
# exactly invariant #1 — a call invalid on the only concrete member
# (`UnionElimUnknownSink`) stays silent because the union already
# degraded to `Unknown` before the elimination ever runs.
class UnionElimUnknownSink
  def only_on_sink_class
  end
end

class UnionElimUnknownWidget
  #: (UnionElimUnknownSink | UnionElimUnknownExternalGemClass) -> void
  def handle(x)
    if x.is_a?(UnionElimUnknownSink)
      x.only_on_sink_class
    else
      x.definitely_not_a_real_method_anywhere
    end
  end
end
