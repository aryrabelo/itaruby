# Bead ita-hzd control (mutant (b), prefix/partial-match guard): the
# vendored RBI declares `RbiHzdRubyLsp::NotificationHandlerExtra` — a real
# class whose full qualified name merely has the bare-referenced name
# (`NotificationHandler`) as a STRING PREFIX. None of the nesting
# candidates this reference's scope walks
# (`RbiHzdRubyLsp::Tapioca::AddonTwo::NotificationHandler`,
# `RbiHzdRubyLsp::Tapioca::NotificationHandler`, `RbiHzdRubyLsp::NotificationHandler`)
# spell that name EXACTLY, so this must keep accusing. A mutant that
# accepts a prefix/substring match instead of `rbi_declares`'s own exact
# string equality would wrongly suppress this.
module RbiHzdRubyLsp
  module Tapioca
    class AddonTwo
      def bad
        NotificationHandler
      end
    end
  end
end
