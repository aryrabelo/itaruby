# Bare (implicit-receiver) calls into Kernel-level DSL methods contributed
# by public gems — must stay silent (invariant #1), never a false E0101.
class GemKernelDslStoplightCaller
  def call_it
    Stoplight("gem-kernel-dsl-circuit") { true }
  end
end

class GemKernelDslRainbowCaller
  def call_it
    Rainbow("gem-kernel-dsl-text").red
  end
end

class GemKernelDslGettextCaller
  def translate
    _("gem-kernel-dsl-msgid")
  end

  def translate_scoped
    s_("gem-kernel-dsl-scope|gem-kernel-dsl-msgid")
  end
end
