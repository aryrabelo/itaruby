# Defect B (bead ita-exc): a top-level constant assignment is fed only into
# the resolution-inert `FileDefs.consts` / `project_consts` map, never into
# `fragments[i].consts` (the bucket `const_exists` actually reads), because
# `DefWalker`'s `ConstantWriteNode` arm only populates `fragments[i].consts`
# when `frag_idx` is `Some` — i.e. inside a class/module body. This file
# just defines the constant at top level; nothing here needs to resolve.
# Silent (paired with toplevel_const_ref.rb, which is where E0104 fires
# today).
CONSTVIS_LIST = [1, 2, 3]
