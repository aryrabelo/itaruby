# typed: true
# DO NOT EDIT MANUALLY — toy fixture for bead ita-3dg, mirroring the real
# RuboCop shape: `RuboCop::Version::STRING = T.let(T.unsafe(nil), String)`
# in a vendored gem .rbi, with `RuboCop::Version` declared as its own
# module header (never nested) and the constant written as a SEPARATE
# fully-qualified value write at file toplevel — never a project alias in
# the way at all. Names are invented, not a real gem.

module Rbi3dgVendoredGem::Version
end

Rbi3dgVendoredGem::Version::STRING = T.let(T.unsafe(nil), String)
