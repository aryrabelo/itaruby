# The mattr_accessor family defines BOTH tracks: the class accessor and,
# unless told otherwise, the instance accessor. Every call below is real at
# runtime (this file exits 0 under MRI) and must resolve silently.
#
# Indexing the macro is ADDITIVE ONLY: it records the accessors and never
# opens or closes the class. Closing is what the first attempt did, and it
# was measured and reverted on 2026-09-17 - on discourse, `TopicQuery`'s
# only class-body opener is `cattr_accessor :results_filter_callbacks`, so
# handling the macro closed the class and produced 28 new E0101 on methods
# that are real, installed from a plugin file by
# `add_to_class(:topic_query, :list_group_topics_assigned)`. Closing a
# class adds no knowledge; it only unmasks what the index cannot see. With
# openness preserved the three public corpora move by exactly zero.
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
  mattr_reader :region
  mattr_writer :token
  cattr_accessor :retries
end

# class-object track
Config.endpoint = "https://example.test"
Config.endpoint
Config.region
Config.token = "t"
Config.retries = 3
Config.retries

# instance track
config = Config.new
config.endpoint = "https://example.test"
config.endpoint
config.region
config.token = "t"
config.retries = 3
config.retries
