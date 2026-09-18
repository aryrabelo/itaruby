class HasExternalBase < ExternalBase
  def run
    helper_from_base
  end
end

class UsesExternalGem
  def run
    ExternalGem::Client.new.fetch
  end
end
