# frozen_string_literal: true

# PRECEDENCE GUARD: the project defines its own `raise` on the class
# object, so the call RESOLVES to the project method — the tail inventory
# must never be consulted, let alone mask a resolvable name. MRI: the
# project method wins and returns its own value.
class Uploader
  def self.raise(*)
    "project-raise"
  end

  def self.flush
    raise "never reached"
  end
end

raise "expected the project raise" unless Uploader.flush == "project-raise"
