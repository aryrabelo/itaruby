# Synthetic-only (not observed on any corpus, but the same code path):
# `ConstVisOuter::ConstVisInner = 42` is a `ConstantPathWriteNode` at top
# level (frag_idx None), captured only into the resolution-inert
# `project_consts` map — same defect B gap as the two `toplevel_*` pairs,
# just via a qualified path instead of a bare name. Silent; paired with
# nested_path_write_ref.rb.
module ConstVisOuter
end

ConstVisOuter::ConstVisInner = 42
