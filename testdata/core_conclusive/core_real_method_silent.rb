# ita-2ve closed-world, SILENT: `center` is NOT in core.rs's allowlist
# (the allowlist alone would call it unknown) but IS in the generated
# inventory (String#center) -> silence. This fixture is the inventory's
# entire reason to exist: it kills the false positive an allowlist-only
# closed world would produce.
class CoreConclusiveCenter
  def pad
    "hello".center(10)
  end
end
