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
  class_attribute :setting, instance_predicate: false
end

Base.setting = true
raise "reader intact" unless Base.setting == true
Base.setting?
