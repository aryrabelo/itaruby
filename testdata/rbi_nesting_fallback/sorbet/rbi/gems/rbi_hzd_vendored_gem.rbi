# typed: true
# DO NOT EDIT MANUALLY — toy fixture for bead ita-hzd, mirroring the real
# tapioca/ruby-lsp shape: `RbiHzdRubyLsp::Notification` declared at its own
# compact, FULLY QUALIFIED path (Tapioca never infers a project's own
# lexical nesting), while the project's real code references it bare
# from deep inside `RbiHzdRubyLsp::Tapioca::Addon`. `RbiHzdRubyLsp::NotificationHandlerExtra`
# exists only to prove the fix's exact-match discipline: a mutant that
# accepts a prefix/substring match instead of an exact qualified-path
# equality would wrongly let it answer a bare `NotificationHandler`
# reference. Names are invented (globally unique `RbiHzd*` prefix per
# AGENTS.md/bead ita-u1t), not the real gem.

module RbiHzdRubyLsp
end

module RbiHzdRubyLsp::Tapioca
end

class RbiHzdRubyLsp::Message
end

class RbiHzdRubyLsp::Notification < ::RbiHzdRubyLsp::Message
end

class RbiHzdRubyLsp::NotificationHandlerExtra
end
