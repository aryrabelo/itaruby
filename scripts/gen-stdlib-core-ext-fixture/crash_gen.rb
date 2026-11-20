# Not a feature probe: a stand-in GENERATOR for the selftest's guard on
# `expect_gen` itself. It prints exactly the inventory `expect_gen` is
# asked for and then fails; output followed by a failure is no inventory,
# so `expect_gen` must refuse it.
puts "Integer#crash_probe_name"
exit 1
