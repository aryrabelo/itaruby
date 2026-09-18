# typed: true
# The declaration Sorbet needs for a `method_missing` forwarder: every name
# the proxy is expected to answer, written out by hand.
class Proxy
  def upcase; end
  def deploy(env); end
end
