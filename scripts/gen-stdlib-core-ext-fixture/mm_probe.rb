# Probe for scripts/gen-stdlib-core-ext.rb (run by
# scripts/audit-curated-gems-selftest.sh via ITA_STDLIB_FEATURES and -I):
# redefines the catch-all hook on a chain class. `method_missing` already
# exists (private, `BasicObject`) before any require, so a name diff alone
# never sees it; the generator must print `Integer#*`.
class Integer
  def method_missing(name, *args) = name
end
