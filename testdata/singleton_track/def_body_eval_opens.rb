# frozen_string_literal: true

# Bead ita-evb: a bare `eval(<string>)` inside a METHOD body defines
# whatever that string says, on the enclosing class — discourse's
# `Middleware::AnonymousCache.compile_key_builder`
# (`lib/middleware/anonymous_cache.rb:33-43`) builds
# `"def self.__compiled_key_builder(h) ... end"` and evals it, which is
# the ONLY definition of that method in the tree. The class-body spelling
# already opened its class; the def-body spelling did not, so the call at
# :47 read as a conclusive miss on code that runs.
class EvbCache
  def self.evb_compile
    eval("def self.evb_compiled_key(h)\n  h\nend") # rubocop:disable Security/Eval
  end

  def self.evb_build(helper)
    evb_compile
    evb_compiled_key(helper)
  end
end

raise "bad" unless EvbCache.evb_build("x") == "x"
