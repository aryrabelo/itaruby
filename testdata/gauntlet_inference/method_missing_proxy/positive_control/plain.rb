# typed: true
# method_missing + respond_to_missing?: the receiver's real surface is
# decided at runtime by whatever @target is. Neither `upcase` nor `deploy`
# is defined on Proxy, and both calls succeed — the only correct static
# verdict is silence.
class Backend
  def deploy(env)
    "deployed to #{env}"
  end
end

class Proxy
  def initialize(target)
    @target = target
  end

  def method_missing(name, *args, &blk)
    if @target.respond_to?(name)
      @target.public_send(name, *args, &blk)
    else
      super
    end
  end

  def respond_to_missing?(name, include_private = false)
    @target.respond_to?(name, include_private) || super
  end
end

Proxy.new("stone").upcase
Proxy.new(Backend.new).deployy("staging")