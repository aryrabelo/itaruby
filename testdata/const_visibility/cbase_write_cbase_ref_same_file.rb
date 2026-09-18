# Bead ita-9he, rails regression shape: a top-level cbase write AND a cbase
# read of the SAME constant in ONE file (`rails_shape.rb`'s
# `::DEFAULT_APP_FILES` measured false-positive). Exercises both the
# write-side fix (index.rs's `ConstantPathWriteNode` arm) and the read-side
# fix (`const_exists`'s empty-`prefix` fallback, which must key
# `toplevel_consts` by the BARE simple name, not the `::`-prefixed text --
# see that fallback's doc comment). Must become silent.
::ConstVisCbaseFiles = %w(app config)

def constvis_print_files
  puts ::ConstVisCbaseFiles
end
