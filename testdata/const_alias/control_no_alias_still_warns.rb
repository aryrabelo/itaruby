# Bead ita-54k G3: negative control — no alias anywhere in this fixture
# set. A genuinely undefined nested constant must keep warning E0104. If
# this ever goes silent, the alias fix has degenerated into "the prefix
# text exists somewhere" instead of "the prefix genuinely resolves".
class ConstAliasControlReader
  def read
    ConstAliasControlUndefined::NESTED
  end
end
