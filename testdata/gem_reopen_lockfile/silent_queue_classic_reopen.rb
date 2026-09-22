# Bead ita-qcn (2026-09-21, dark-census measurement): rails' own test
# support reopens `module QC` (`activejob/test/support/queue_classic/
# inline.rb:4`) only to redefine three `QC::Queue` methods, and then calls
# `QC.default_conn_adapter` — a method the real `queue_classic` gem owns.
# The lock name (`queue_classic`) and the constant (`QC`) share no letters,
# so `gem_namespace_key` cannot pair them; the override table carries the
# mapping, read out of the gem's own source (`lib/queue_classic.rb:5`,
# version 4.0.0, the version this directory's `Gemfile.lock` declares).
module QC
  class Queue
    def enqueue(method)
      method
    end
  end
end

QC.default_conn_adapter
