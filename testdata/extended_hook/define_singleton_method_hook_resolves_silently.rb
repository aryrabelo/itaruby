# Bead ita-dsm: the SINGLETON spelling of a `self.extended(base)` install.
# `base.define_singleton_method(:dsm_values) { ... }` puts the name on the
# EXTENDER's class object — the track `extend` dispatches on — so a call
# to it resolves. discourse's `Migrations::Enum`
# (`migrations/core/lib/migrations/common/enum.rb`) is the measured shape.
module DsmEnum
  def self.extended(base)
    base.define_singleton_method(:dsm_values) { [1, 2] }
  end
end

module DsmMentionType
  extend DsmEnum
end

raise 'the hook must really install it' unless DsmMentionType.dsm_values == [1, 2]
