# `instance_writer: false` removes only the instance writer: the instance
# READER and both class-track accessors stay real. MRI runs the reads clean
# and raises only on the instance assignment.
# A faithful miniature of ActiveSupport's macro, so MRI itself proves what
# this fixture claims: the class accessor always exists, the instance side
# obeys instance_accessor / instance_reader / instance_writer.
class Module
  def mattr_accessor(*names, instance_accessor: true, instance_reader: instance_accessor,
                     instance_writer: instance_accessor, reader: true, writer: true)
    names.each do |n|
      singleton_class.define_method(n) { instance_variable_get("@#{n}") } if reader
      singleton_class.define_method("#{n}=") { |v| instance_variable_set("@#{n}", v) } if writer
      define_method(n) { self.class.public_send(n) } if reader && instance_reader
      define_method("#{n}=") { |v| self.class.public_send("#{n}=", v) } if writer && instance_writer
    end
  end

  def mattr_reader(*names, **opts)
    mattr_accessor(*names, writer: false, **opts)
  end

  def mattr_writer(*names, **opts)
    mattr_accessor(*names, reader: false, **opts)
  end

  alias cattr_accessor mattr_accessor
end

class Config
  mattr_accessor :endpoint, instance_writer: false
end

Config.endpoint = "https://example.test"
Config.endpoint
Config.new.endpoint
Config.new.endpoint = "nope"
