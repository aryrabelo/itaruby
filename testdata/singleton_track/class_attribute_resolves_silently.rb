# A faithful inline re-implementation of ActiveSupport's
# `class_attribute` option chain (read out of the gem's own source at
# core_ext/class/attribute.rb) so the fixture runs under plain MRI with
# no gems: the singleton reader/writer always, the `a?` predicate
# (singleton side always, instance side with the reader), the instance
# reader/writer behind `instance_reader`/`instance_writer`, each
# defaulting to `instance_accessor`.
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
  class_attribute :setting

  def self.report
    setting?
  end
end

raise "reader" unless Base.setting.nil?
Base.setting = true
raise "writer" unless Base.setting == true
raise "predicate" unless Base.report == true
raise "instance reader" unless Base.new.setting == true
raise "instance writer" unless (Base.new.setting = false) == false
raise "instance predicate" unless Base.new.setting? == false
