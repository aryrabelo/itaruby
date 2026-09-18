# Bead ita-4wq: `Constant::MessageType::WARNING`, referenced bare inside
# `Rbi4wqRubyLsp::Tapioca::SomeAddon`, needs the FULL chain: bead ita-hzd's
# nesting expansion finds the candidate
# `Rbi4wqRubyLsp::Constant::MessageType::WARNING`; THIS bead recognizes
# `Rbi4wqRubyLsp::Constant` as an alias EDGE written inside the vendored
# RBI itself, whose target is `Rbi4wqLanguageServer::Protocol::Constant`;
# beads ita-3dg/ita-y0s's `rbi_qualified_const_declares` then resolves
# `WARNING` as a qualified `T.let` write under the expanded
# `Rbi4wqLanguageServer::Protocol::Constant::MessageType` namespace.
# `bad` names a member the RBI genuinely never declares under the
# resolved target (anti-suppression control). `bad_method_call` is the
# suppression-only proof (mutant (c)): even through a resolved alias
# edge, the reference still types `Ty::Unknown` — an obviously undefined
# method call on it must never raise E0101.
module Rbi4wqRubyLsp
  module Tapioca
    class SomeAddon
      def handle
        Constant::MessageType::WARNING
      end

      def bad
        Constant::MessageType::NOPE
      end

      def bad_method_call
        Constant::MessageType::WARNING.this_method_does_not_exist
      end
    end
  end
end
