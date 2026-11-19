# Probe: `respond_to_missing?` already exists (private, `Kernel`) on every
# chain class; redefining it on String must print `String#*`.
class String
  def respond_to_missing?(name, include_private = false) = true
end
