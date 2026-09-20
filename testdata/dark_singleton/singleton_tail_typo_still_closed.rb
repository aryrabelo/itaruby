# frozen_string_literal: true

# The accusation must survive the tail: a TYPO inside a `def self.` body is
# not a tail name, so it stays in the residue bucket (closed_notfound) —
# exactly what a class-object E0101 fires on once the track arms. MRI runs
# this file to completion: `performm` is defined but never invoked.
class Job
  def self.perform
    performm
  end
end
