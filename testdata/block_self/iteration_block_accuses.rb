# Bead ita-uye, the ACCUSING side. `Integer#times` and `String#each_char`
# merely yield, so `self` inside their blocks is the enclosing instance with
# certainty: a self-send of a method this class never defines is a real
# dormant NameError, and E0101 is conclusive again here (bead ita-4xy had
# softened every in-block self-send).
#
# Receiver choice is load-bearing: `ita check testdata/` runs the whole tree
# as ONE merged project, and testdata/core_conclusive/reopened_core_silent.rb
# reopens `Array` — which correctly disqualifies every Array receiver in the
# tree. Integer and String are unpolluted here, so they are what proves the
# capability under gate c. The pollution side is proven in isolation by
# crates/itaruby_semantic/tests/block_self.rs.
class BlkSelfIterator
  def run
    3.times { blkself_missing_from_times }
    "abc".each_char { |c| blkself_missing_from_each_char(c) }
  end
end
