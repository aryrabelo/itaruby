# typed: true
# A module's singleton reopened through class_eval + define_method: the
# class method `generate` exists at runtime and in no `def` in this file.
module Report
end

Report.singleton_class.class_eval do
  define_method(:generate) { "ok" }
end

Report.generatte