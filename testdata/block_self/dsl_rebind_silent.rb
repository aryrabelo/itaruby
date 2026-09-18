# Bead ita-uye, the MANDATORY COUNTER-PROOF. This is the corpus shape that
# made bead ita-4xy soften in-block self-sends in the first place: a DSL
# class that `instance_exec`s the block it was handed, so `self` inside that
# block is the context object and not the lexically enclosing instance.
#
# `ctx.field` is a PROJECT method, so it is never proven lexical (only the
# core allowlist proves anything today) and the block stays soft. Silence is
# the correct answer: `blkself_context_only` may well be defined on the
# context object at runtime. Invariant #1 — a false negative here, never a
# false positive.
class BlkSelfContext
  def initialize
    @fields = {}
  end

  def field(name, &blk)
    @fields[name] = instance_exec(&blk)
  end

  def blkself_context_only
    42
  end
end

class BlkSelfSchema
  def build
    ctx = BlkSelfContext.new
    ctx.field(:total) { blkself_context_only }
  end
end
