# Real-world shape (bead ita-ekg): zammad's
# lib/core_ext/mail/fields/common_date_field.rb:15 —
# `class CommonDateField < NamedStructuredField #:nodoc:`. The RDoc
# visibility directive `#:nodoc:` glues directly onto `#:` with no space,
# trailing the class line, immediately followed by a `def` on the very
# next line. It must never be mistaken for a signature comment attaching
# to that def (no E0105, no sig).
class RdocDirNoDocParent
end

class RdocDirNoDocChild < RdocDirNoDocParent #:nodoc:
  def greet(name)
    name.to_s
  end
end
