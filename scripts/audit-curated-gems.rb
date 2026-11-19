#!/usr/bin/env ruby
# W11 (Tapioca open world): source audit of the gems Tapioca never writes an
# RBI for (`open_world::CURATED_GEMS`). Prints, per unpacked gem dir, every
# reopening of a core-chain namespace, every chain-receiver mixin or method
# injection, every top-level `def`/`alias` and every top-level
# `include`/`prepend`/`define_method` (main delegates them to `Object`),
# every `refine`, every `eval` whatever its receiver, and every type-test /
# `method_missing` definition. A `def`/`alias`/mixin inside `class << ...`
# is printed as `singleton_*` (`Owner.singleton`), never as an instance
# method: the reader decides. Its findings are the names `CURATED_GEMS`
# lists; a new version enters that table only after this audit over
# `gem unpack <name> -v <version>` reads clean.
#
# Scanned: every `.rb` under `lib/` and `exe/`, plus every extensionless
# file under `exe/` whose first line is a Ruby shebang. Any other file
# under `exe/` is printed as `skipped_non_ruby_exe`, never silently
# dropped. A file Prism cannot parse aborts the audit (exit 1, file named
# on stderr): a partial parse would read clean.
#
#   ruby scripts/audit-curated-gems.rb <unpacked-gem-dir>...
require "prism"
CHAIN = %w[Integer Float Numeric Comparable String Symbol Array Hash Enumerable NilClass TrueClass FalseClass Object Kernel BasicObject]
DYN = %i[include prepend class_eval module_eval class_exec module_exec instance_exec define_method send public_send __send__ instance_eval alias_method attr_accessor attr_reader attr_writer refine]
# Bare calls at top level that `main` forwards to `Object` (instance
# surface of every object) or applies to its own singleton (`extend`).
TOPLEVEL_OBJECT = %i[include prepend define_method]
SINGLETON = ".singleton"

class V < Prism::Visitor
  attr_reader :out
  def initialize(file, out) = (@file, @out, @stack = file, out, [])
  def cname(n)
    case n
    when Prism::ConstantReadNode then n.name.to_s
    when Prism::ConstantPathNode then n.parent.nil? ? "::#{n.name}" : "#{cname(n.parent)}::#{n.name}"
    end
  end
  def chain_of(node)
    nm = cname(node.constant_path)
    return nil unless nm
    bare = nm.delete_prefix("::")
    (@stack.empty? || nm.start_with?("::")) && CHAIN.include?(bare) ? bare : nil
  end
  def visit_class_node(n) = scope(n) { super }
  def visit_module_node(n) = scope(n) { super }
  def scope(n)
    c = chain_of(n)
    @stack.push(c || cname(n.constant_path) || "?")
    @chain = c ? c : @chain
    yield
  ensure
    @stack.pop
  end
  # `class << self` / `class << Const`: defs land on that object's
  # singleton class, never on the enclosing class's instances.
  def visit_singleton_class_node(n)
    base =
      case n.expression
      when Prism::SelfNode then @stack.empty? ? "main" : @stack.last
      else cname(n.expression)&.delete_prefix("::") || "?"
      end
    @stack.push("#{base}#{SINGLETON}")
    super
  ensure
    @stack.pop
  end
  # The singleton scope worth printing: one of a chain class, or of main.
  def singleton_owner
    top = @stack.last.to_s
    return nil unless top.end_with?(SINGLETON)
    base = top.delete_suffix(SINGLETON)
    CHAIN.include?(base) || base == "main" ? top : nil
  end
  def loc(n) = "#{@file}:#{n.location.start_line}"
  def record_def(name, n)
    owner = @stack.last
    if @stack.empty? then out << [:toplevel_def, name, loc(n)]
    elsif CHAIN.include?(owner) then out << [:chain_def, "#{owner}##{name}", loc(n)]
    elsif (s = singleton_owner) then out << [:singleton_def, "#{s}##{name}", loc(n)]
    end
  end
  def visit_def_node(n)
    if n.receiver.nil?
      record_def(n.name, n)
      out << [:faker_def, "#{@stack.join('::')}##{n.name}", loc(n)] if %i[is_a? kind_of? instance_of? method_missing respond_to_missing?].include?(n.name)
    end
    super
  end
  # `alias new old` (the keyword) defines `new` exactly like `alias_method`.
  def visit_alias_method_node(n)
    nn = n.new_name
    record_def(nn.respond_to?(:unescaped) ? nn.unescaped : nn.slice, n)
    super
  end
  def visit_call_node(n)
    r = n.receiver
    if DYN.include?(n.name)
      rn = r && (r.is_a?(Prism::ConstantReadNode) || r.is_a?(Prism::ConstantPathNode)) ? cname(r)&.delete_prefix("::") : nil
      if rn && CHAIN.include?(rn) then out << [:chain_call, "#{rn}.#{n.name}", loc(n)]
      elsif r.nil? && CHAIN.include?(@stack.last.to_s) then out << [:chain_body_call, "#{@stack.last} #{n.name}", loc(n)]
      elsif r.nil? && @stack.empty? && TOPLEVEL_OBJECT.include?(n.name) then out << [:chain_body_call, "Object #{n.name}", loc(n)]
      elsif r.nil? && (s = singleton_owner) then out << [:singleton_body_call, "#{s} #{n.name}", loc(n)]
      elsif n.name == :refine then out << [:refine, n.slice[0, 60], loc(n)]
      elsif rn.nil? && r && %i[include prepend class_eval module_eval define_method send].include?(n.name) && r.slice =~ /\b(#{CHAIN.join('|')})\b/
        out << [:chain_call_indirect, n.slice[0, 80], loc(n)]
      end
    end
    out << [:singleton_body_call, "main#{SINGLETON} extend", loc(n)] if n.name == :extend && r.nil? && @stack.empty?
    out << [:eval, n.slice[0, 60], loc(n)] if n.name == :eval
    super
  end
end

def ruby_exe?(path)
  return true if path.end_with?(".rb")
  line = File.open(path, "rb") { |io| io.gets }.to_s
  line.start_with?("#!") && line.include?("ruby")
end

ARGV.each do |dir|
  out = []
  lib = Dir.glob("#{dir}/lib/**/*.rb").select { |f| File.file?(f) }
  exe = Dir.glob("#{dir}/exe/**/*").select { |f| File.file?(f) }
  exe_ruby, exe_other = exe.partition { |f| ruby_exe?(f) }
  (lib + exe_ruby).sort.each do |f|
    res = Prism.parse_file(f)
    unless res.errors.empty?
      e = res.errors.first
      abort("audit-curated-gems: parse error in #{f}:#{e.location.start_line}: #{e.message}")
    end
    V.new(f.delete_prefix("#{dir}/"), out).visit(res.value)
  end
  exe_other.sort.each do |f|
    out << [:skipped_non_ruby_exe, File.open(f, "rb") { |io| io.gets }.to_s.chomp[0, 60].inspect, f.delete_prefix("#{dir}/")]
  end
  puts "### #{File.basename(dir)} (#{lib.size} lib .rb, #{exe_ruby.size} exe Ruby, #{exe_other.size} exe skipped)"
  out.each { |k, v, l| puts "  #{k}\t#{v}\t#{l}" }
end
