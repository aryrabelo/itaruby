# Defect B, the `Name = Class.new(StandardError)` custom-exception idiom
# (measured on corpus-b: 2 roots / 11 occurrences). Same
# top-level bucket gap as toplevel_const_def.rb, just a different RHS
# shape. Silent; paired with toplevel_class_new_ref.rb.
ConstVisSentinel = Class.new(StandardError)
