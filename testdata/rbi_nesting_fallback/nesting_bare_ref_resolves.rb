# Bead ita-hzd: `Notification`, referenced BARE from 3 levels deep inside
# `RbiHzdRubyLsp::Tapioca::Addon`, must resolve through the vendored RBI's
# compact-form `RbiHzdRubyLsp::Notification` header exactly as Ruby's own
# lexical cref lookup would find it — walking `Addon`, then `Tapioca`,
# then `RbiHzdRubyLsp` itself, innermost first, until a level's own
# `<level>::Notification` matches something the RBI actually declares.
# `bad` names a constant that exists at NO nesting level this reference
# could ever walk (anti-suppression control).
module RbiHzdRubyLsp
  module Tapioca
    class Addon
      def handle
        Notification
      end

      def bad
        NeverDeclaredAtAnyNestingLevel
      end
    end
  end
end
