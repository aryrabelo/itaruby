# Inline thread_mattr_accessor (active_support/core_ext/module/
# attribute_accessors_per_thread.rb): singleton reader/writer always,
# instance reader behind instance_reader && instance_accessor, instance
# writer behind instance_writer && instance_accessor.
class Class
  def thread_mattr_accessor(*syms, instance_reader: true,
    instance_writer: true, instance_accessor: true)
    syms.each do |name|
      define_singleton_method(name) { instance_variable_get(:"@_#{name}") }
      define_singleton_method(:"#{name}=") { |v| instance_variable_set(:"@_#{name}", v) }
      if instance_reader && instance_accessor
        define_method(name) { self.class.send(name) }
      end
      if instance_writer && instance_accessor
        define_method(:"#{name}=") { |v| self.class.send(:"#{name}=", v) }
      end
    end
  end
end

class Job
  thread_mattr_accessor :queue

  def self.drain
    queue
  end
end

Job.queue = :low
raise "class reader" unless Job.drain == :low
Job.queue = :high
raise "instance reader" unless Job.new.queue == :high
Job.new.queue = :critical
raise "instance writer" unless Job.queue == :critical
