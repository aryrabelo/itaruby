# typed: true
# attr_reader is a method-defining macro, not a declaration: the reader
# `email` exists, `emial` does not.
class Customer
  attr_reader :email

  def initialize(email)
    @email = email
  end
end

Customer.new("a@b.test").emial