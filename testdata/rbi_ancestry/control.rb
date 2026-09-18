# W3 control: same external-ancestry shape, but the referenced constant
# exists in NO RBI — the external walk is a resolution path, not a
# blanket suppressor, so this keeps warning E0104 even with the RBI
# wired.
class RbiAncControl < SynthGem::Schema::Object
  def m(x)
    NotInAnyRbiConst
  end
end
