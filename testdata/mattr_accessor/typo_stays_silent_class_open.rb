# KNOWN LIMITATION, measured 2026-09-17, not a target. `Config.new.endpoin`
# is a real NoMethodError (MRI raises here) and itaruby stays SILENT.
# The reason is class openness, not the macro: indexing `mattr_accessor`
# deliberately leaves the class exactly as open as it was before the macro
# was understood, and a lookup on an open class is Inconclusive, never
# NotFound (invariant #1).
#
# Closing the class instead is what the first attempt did, and it was
# measured and reverted the same day: on discourse, `TopicQuery`'s only
# class-body opener is `cattr_accessor :results_filter_callbacks`, so
# closing it produced 28 new E0101 on methods that are real - installed
# from a plugin file by `add_to_class(:topic_query, :list_group_topics_assigned)`,
# which no index here can model. Closing a class adds no knowledge; it
# only unmasks what the index already could not see. So this file records
# the silence as a known gap rather than pretending it is a diagnostic.
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
  mattr_accessor :endpoint
end

Config.new.endpoin
