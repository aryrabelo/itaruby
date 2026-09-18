# Bead ita-47y (RBI-target extension): `read` must resolve exactly as
# `ConstAlias47yVendoredGem::Interface::CompletionItemKind::FIELD` would
# through the vendorized gem's own RBI — silent, no E0104. `bad` names a
# member the RBI genuinely never declares at that expanded path and must
# keep warning E0104 (anti-suppression control: the alias-to-RBI bridge
# must never become a blanket suppressor for everything under the
# aliased namespace).
class ConstAlias47yRbiReader
  def read
    ConstAlias47yRbiBridge::CompletionItemKind::FIELD
  end

  def bad
    ConstAlias47yRbiBridge::CompletionItemKind::NOPE
  end
end
