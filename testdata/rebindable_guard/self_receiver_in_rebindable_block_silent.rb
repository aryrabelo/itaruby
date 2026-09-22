# Bead ita-slf: an EXPLICIT `self` receiver inside a block whose receiving
# call can rebind `self` names the REBOUND object, never the lexically
# enclosing class. discourse writes both measured shapes:
# `base.define_method(:chat_send_shortcut=) { |v| self.send_shortcut = v }`
# (`plugins/chat/lib/chat/user_option_extension.rb:116`) and
# `Class.new(Command) do self.description = "..." end`
# (`migrations/core/lib/migrations/cli/bootstrap.rb:62`). Both were census
# residue records blamed on a class object that never sees the call —
# here `Extension`, whose own class object has no `slf_send_shortcut=`.
module SlfRebindableSelfSilent
  class Option
    attr_accessor :slf_send_shortcut
  end

  class Extension
    def self.slf_install(base)
      base.define_method(:slf_chat_shortcut=) { |value| self.slf_send_shortcut = value }
    end
  end
end

SlfRebindableSelfSilent::Extension.slf_install(SlfRebindableSelfSilent::Option)
opt = SlfRebindableSelfSilent::Option.new
opt.send(:slf_chat_shortcut=, 7)
raise 'the rebound self must have received the write' unless opt.slf_send_shortcut == 7
