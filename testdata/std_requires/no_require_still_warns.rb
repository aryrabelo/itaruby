# W3 control: `ERB` IS a stdlib constant (harvest: `require 'erb'`
# defines it), but no file in this project requires 'erb' — the gate must
# NOT fire on the name alone, so this reference keeps warning E0104. A gem
# in the real app could require it transitively; that stays a false
# negative forever (warnings only fall, invariant #1), never a guess.
class StdReqNoRequire
  def render(text)
    ERB.new(text).result
  end
end
