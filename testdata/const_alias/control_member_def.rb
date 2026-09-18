# Bead ita-47y control: the alias itself (`ConstAlias47yControlBridge`)
# resolves to a REAL, closed project class — a member genuinely absent
# from it must still accuse, both for a missing nested constant (E0104)
# and for a missing INSTANCE method call reached by instantiating the
# class through the alias (E0101 — this checker deliberately never
# diagnoses an unknown CLASS/singleton method, see `check.rs`'s `Ty::Class`
# arm, so the E0101 half of this control instantiates first). Anti-
# suppression: alias resolution must never degenerate into a blanket
# suppressor for everything reached through it — only a segment that
# genuinely resolves may ever silence a diagnostic.
module ConstAlias47yControlTarget
  class Real
    def known_method
      1
    end
  end
end

ConstAlias47yControlBridge = ConstAlias47yControlTarget
