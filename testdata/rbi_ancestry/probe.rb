# W3 external RBI ancestry: `IDENT` is defined only on the mixin the gem
# puts in the superclass chain (`SynthGem::TypeNames`, included by
# `SynthGem::Schema::Member`, superclass of `SynthGem::Schema::Object`).
# Under `ita check testdata/` this KEEPS warning — the nested sorbet/rbi
# is not discovered from the parent root (per-root discovery) — and the
# library test wires the RBI explicitly to prove the silence.
class RbiAncUsesMixin < SynthGem::Schema::Object
  def m(x)
    IDENT
  end
end
