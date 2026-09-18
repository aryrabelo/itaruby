# typed: true
# The declaration Sorbet needs for a singleton reopened through
# `class_eval` + `define_method`.
module Report
  def self.generate; end
end
