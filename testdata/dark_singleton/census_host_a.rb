# frozen_string_literal: true

# Bead ita-census, host side: this file CALLS into census_host_b.rb. Its
# own walk has no census-able site — every record this file's census
# produces would be a foreign-span ghost from the nested body walk.
x = M.outer(1)
