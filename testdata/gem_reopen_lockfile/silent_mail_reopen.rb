# Bead ita-547 (Round-5 audit, gitlab/rails sites): an initializer reopens
# `Mail::SMTP` to override delivery. `Mail::SMTP` is NOT in the curated
# `gems.rbi` allowlist (only `Mail::Message`/`Mail::Address` are — mechanism
# B, bead ita-h6l) and this file never defines `humanized_delivery_status`
# anywhere, so before this bead every such reopening turned every OTHER
# method the real `mail` gem defines into a false E0101. `mail` is declared
# in this directory's `Gemfile.lock`, so the class must be treated as
# unknown ancestry and stay silent.
class Mail::SMTP
  def deliver!(mail)
    true
  end
end

Mail::SMTP.new.humanized_delivery_status
