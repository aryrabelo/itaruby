# Bead ita-qcn (2026-09-21, dark-census measurement): rails' own test
# support reopens `module QC` (`activejob/test/support/queue_classic/
# inline.rb:4`) only to redefine three `QC::Queue` methods, and then calls
# `QC.default_conn_adapter` — a method the real `queue_classic` gem owns.
# The lock name (`queue_classic`) and the constant (`QC`) share no letters,
# so `gem_namespace_key` cannot pair them; the override table carries the
# mapping, read out of the gem's own source (`lib/queue_classic.rb:5`,
# version 4.0.0, the version this directory's `Gemfile.lock` declares).
#
# Issue #6 (2026-10-06): rails' exact shape - a module this tree writes only
# as a namespace - is now also opened by the namespace-only pass, which made
# CO-L (the override entry cut) BLIND. This reopening therefore also writes
# a module method of its own, the shape where the tree owns part of `QC`'s
# surface and only the lockfile mapping can say the rest lives in the gem.
module QC
  def self.inline?
    true
  end

  class Queue
    def enqueue(method)
      method
    end
  end
end

QC.default_conn_adapter
