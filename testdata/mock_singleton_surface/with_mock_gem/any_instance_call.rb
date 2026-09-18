class Invoice
  def total
    0
  end
end

# The measured discourse shape: a mocking call on a class object, from
# spec code. MRI cannot execute this file without the gems; the runtime
# facts it stands for were verified on this machine directly:
#   ruby -e 'require "rspec/mocks"; class Zed; end; Zed.any_instance'
#     -> RSpec::Mocks::AnyInstance::Proxy
#   ruby -e 'class Zed; end; Zed.any_instance'
#     -> NoMethodError
Invoice.any_instance
