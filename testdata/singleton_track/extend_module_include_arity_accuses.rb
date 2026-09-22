# frozen_string_literal: true

# Bead ita-xta, ACCUSATION side: the transitively extended method keeps
# its real signature, so the wrong-arity call is an E0102 MRI raises.
module XtaArityNaming
  def xta_human(scope)
    scope
  end
end

module XtaArityTranslation
  include XtaArityNaming
end

class XtaArityGender
  extend XtaArityTranslation
end

XtaArityGender.xta_human
