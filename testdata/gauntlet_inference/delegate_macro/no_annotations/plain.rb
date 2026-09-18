# typed: true
# The Rails `delegate` shape, written out: a class macro on Module that
# defines the forwarding methods with define_method. `display_name` and
# `timezone` are never written as `def` anywhere on Account.
class Module
  def delegate(*names, to:)
    names.each do |name|
      define_method(name) do |*args, &blk|
        send(to).public_send(name, *args, &blk)
      end
    end
  end
end

class Profile
  def display_name
    "ita"
  end

  def timezone
    "UTC"
  end
end

class Account
  delegate :display_name, :timezone, to: :profile

  def profile
    Profile.new
  end
end

Account.new.display_name
Account.new.timezone