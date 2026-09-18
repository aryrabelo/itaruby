# Same planted typo as ../narrowing_core_typo.rb, behind a Gemfile: fires
# when testdata/ is the checked root (the Gemfile is below it, discovery
# never looks down), silent when this directory is the checked root.
class CoreConclusiveGatedAppender
  def append(x)
    if x.is_a?(String)
      x.pushh(1)
    end
    x
  end
end
