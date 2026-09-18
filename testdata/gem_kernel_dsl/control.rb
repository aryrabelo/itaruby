# Negative control: a genuinely inexistent bare method name, outside the
# curated GEM_KERNEL_METHODS allowlist, must still accuse E0101 — proves
# the allowlist is a narrow allowlist, not a blanket softening.
class GemKernelDslControl
  def call_it
    gem_kernel_dsl_totally_unknown_bare_call("x")
  end
end
