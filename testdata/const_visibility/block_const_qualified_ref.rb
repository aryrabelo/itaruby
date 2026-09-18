# Defect A, cross-file: referencing `ConstVisEnum::ConstVisAlpha` by
# qualified path from a different file in the same project. Must become
# silent — same underlying fix as block_const_def.rb's bare self-reference.
class ConstVisConsumer
  def label
    ConstVisEnum::ConstVisAlpha
  end
end
