# Scoped to modules, the only shape the FP-B measurement found (13
# receivers, 2534 sites, every one a module): a class holding only nested
# definitions, here its own error class, keeps accusing until a corpus shows
# a gem class reopened this way. CO-V drops the module check and must
# silence 11:7.
class Relay
  class Error < StandardError
  end
end

Relay.connect
