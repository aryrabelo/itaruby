# Bead ita-h5s control: `FakeConst` only ever appears as a HEREDOC BODY
# line in the vendored RBI (`DESC = <<~TEXT\nFakeConst = 1\nTEXT`), never
# as a real `ConstantWriteNode`. Phase 1's cheap line scan cannot tell
# the difference and DOES produce a candidate file for it; only phase
# 2's real Prism parse (`FileDefs::toplevel_consts`) correctly rejects
# it. This reference must still emit E0104 — if `rbi_toplevel_const_declares`
# ever degraded to "the candidate file exists" instead of checking the
# parsed `toplevel_consts`/`Object`/`Kernel` fragment membership, this
# would wrongly go silent.
class Rbih5sHeredocControl
  def bad
    FakeConst
  end
end
