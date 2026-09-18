# `kt-paperclip` -> `Paperclip`: an exception the key cannot derive,
# because the namespace differs in LETTERS, not in case. Verified in the
# gem's own source: kt-paperclip 8.0.0, `lib/paperclip.rb:82`.
module Paperclip
  class Attachment
    def custom_style
      styles
    end
  end
end

Paperclip::Attachment.new.custom_style
