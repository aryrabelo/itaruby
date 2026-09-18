# The inline implementation mirrors the gem's semantics for a DYNAMIC
# option value: at runtime `ARGV[0]` decides, so every side may exist.
# The checker reads the non-literal option conservatively the same way
# (defines everything): it can never prove the reader absent.
class Class
  def class_attribute(*attrs, instance_accessor: true,
    instance_reader: instance_accessor, instance_writer: instance_accessor,
    instance_predicate: true)
    attrs.each do |name|
      define_singleton_method(name) { instance_variable_get(:"@#{name}") }
      define_singleton_method(:"#{name}=") { |v| instance_variable_set(:"@#{name}", v) }
      if instance_predicate
        define_singleton_method(:"#{name}?") { !!instance_variable_get(:"@#{name}") }
      end
      if instance_reader
        define_method(name) { self.class.send(name) }
        define_method(:"#{name}?") { !!self.class.send(name) } if instance_predicate
      end
      if instance_writer
        define_method(:"#{name}=") { |v| self.class.send(:"#{name}=", v) }
      end
    end
  end
end

class Base
  class_attribute :setting, instance_reader: (ARGV[0] != "off")
end

Base.setting = true
raise "class side intact" unless Base.setting == true
if ARGV[0] != "off"
  raise "instance reader present" unless Base.new.setting == true
end
