# The discourse shape (script/import_scripts/base.rb:131-141, generalized):
# a class-body loop with a block — `%i[...].each { |m| delegate m, to: :@lookup }`
# — runs at class-body level and delegates real methods into existence via
# Forwardable at load time. The walker must open the class for a class-body
# call WITH a receiver carrying a block, exactly as it already does for a
# receiverless one; a closed class here makes every delegated name look
# provably missing. Expected: ZERO diagnostics (the class is open; `foo`
# cannot be proven absent).
class DelegateLoopImporter
  %i[foo bar].each { |m| delegate m, to: :@lookup }

  def initialize(lookup)
    @lookup = lookup
  end

  def render_thing
    foo
  end
end

DelegateLoopImporter.new(nil).render_thing
