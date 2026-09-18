# One class per shape `body_def_reason` reacts to. Each must end up OPEN,
# and each line here really runs: the prefilter that decides whether the
# AST walk happens at all is only as good as this list (measured: a
# missing `instance_eval` cost 10 discourse findings).
class ByDefineMethod
  def self.install(name)
    define_method(name) { 1 }
  end
end

class ByDefineSingletonMethod
  def self.install(name)
    define_singleton_method(name) { 2 }
  end
end

class ByAliasMethod
  def self.base
    3
  end

  def self.install(from, to)
    singleton_class.alias_method(to, from)
  end
end

class ByAttr
  def self.install(name)
    attr_accessor name
  end
end

class ByClassEval
  def self.install(name)
    class_eval { define_method(name) { 4 } }
  end
end

class ByInstanceEval
  def self.install(name)
    instance_eval { define_method(name) { 5 } }
  end
end

module ByModuleEval
  def self.install(name)
    module_eval { define_method(name) { 6 } }
  end
end

class ByInstanceExec
  def self.install(blk)
    instance_exec(&blk)
  end
end

class ByClassExec
  def self.install(blk)
    class_exec(&blk)
  end
end

module ByModuleExec
  def self.install(blk)
    module_exec(&blk)
  end
end

ByDefineMethod.install(:a)
ByDefineSingletonMethod.install(:b)
ByAliasMethod.install(:base, :also_base)
ByAttr.install(:c)
ByClassEval.install(:d)
ByInstanceEval.install(:e)
ByModuleEval.install(:f)
ByInstanceExec.install(proc do
  raise "instance_exec must rebind self to the class" unless self == ByInstanceExec
  @via_instance_exec = 7
end)
ByClassExec.install(proc do
  raise "class_exec must rebind self to the class" unless self == ByClassExec
  @via_class_exec = 8
end)
ByModuleExec.install(proc do
  raise "module_exec must rebind self to the module" unless self == ByModuleExec
  @via_module_exec = 9
end)

raise "define_method" unless ByDefineMethod.new.a == 1
raise "define_singleton_method" unless ByDefineSingletonMethod.b == 2
raise "alias_method" unless ByAliasMethod.also_base == 3
raise "attr" unless ByAttr.new.respond_to?(:c)
raise "class_eval" unless ByClassEval.new.d == 4
raise "instance_eval" unless ByInstanceEval.new.e == 5
raise "module_eval" unless Class.new { include ByModuleEval }.new.f == 6
raise "instance_exec" unless ByInstanceExec.instance_variable_get(:@via_instance_exec) == 7
raise "class_exec" unless ByClassExec.instance_variable_get(:@via_class_exec) == 8
raise "module_exec" unless ByModuleExec.instance_variable_get(:@via_module_exec) == 9
