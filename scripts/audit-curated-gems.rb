#!/usr/bin/env ruby
# W11 (Tapioca open world): source audit of the gems Tapioca never writes an
# RBI for (`open_world::CURATED_GEMS`). Prints, per unpacked gem dir, every
# reopening of a core-chain namespace, every chain-receiver mixin or method
# injection, every top-level `def`, `refine`, string `eval`, and every
# type-test / `method_missing` definition. Its findings are the names
# `CURATED_GEMS` lists; a new version enters that table only after this
# audit over `gem unpack <name> -v <version>` reads clean.
#
#   ruby scripts/audit-curated-gems.rb <unpacked-gem-dir>...
require "prism"
CHAIN = %w[Integer Float Numeric Comparable String Symbol Array Hash Enumerable NilClass TrueClass FalseClass Object Kernel BasicObject]
DYN = %i[include prepend class_eval module_eval class_exec module_exec define_method send public_send __send__ instance_eval alias_method attr_accessor attr_reader attr_writer refine]

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
  def loc(n) = "#{@file}:#{n.location.start_line}"
  def visit_def_node(n)
    owner = @stack.last
    if n.receiver.nil?
      if @stack.empty? then out << [:toplevel_def, n.name, loc(n)]
      elsif CHAIN.include?(owner) then out << [:chain_def, "#{owner}##{n.name}", loc(n)]
      end
      out << [:faker_def, "#{@stack.join('::')}##{n.name}", loc(n)] if %i[is_a? kind_of? instance_of? method_missing respond_to_missing?].include?(n.name)
    end
    super
  end
  def visit_call_node(n)
    if DYN.include?(n.name)
      r = n.receiver
      rn = r && (r.is_a?(Prism::ConstantReadNode) || r.is_a?(Prism::ConstantPathNode)) ? cname(r)&.delete_prefix("::") : nil
      if rn && CHAIN.include?(rn) then out << [:chain_call, "#{rn}.#{n.name}", loc(n)]
      elsif r.nil? && CHAIN.include?(@stack.last.to_s) then out << [:chain_body_call, "#{@stack.last} #{n.name}", loc(n)]
      elsif n.name == :refine then out << [:refine, n.slice[0, 60], loc(n)]
      elsif rn.nil? && r && %i[include prepend class_eval module_eval define_method send].include?(n.name) && r.slice =~ /\b(#{CHAIN.join('|')})\b/
        out << [:chain_call_indirect, n.slice[0, 80], loc(n)]
      end
    end
    out << [:eval, n.slice[0, 60], loc(n)] if n.name == :eval && n.receiver.nil?
    super
  end
end

ARGV.each do |dir|
  out = []
  Dir.glob("#{dir}/{lib,exe}/**/*.rb").sort.each do |f|
    res = Prism.parse_file(f)
    V.new(f.delete_prefix("#{dir}/"), out).visit(res.value)
  end
  puts "### #{File.basename(dir)} (#{Dir.glob("#{dir}/lib/**/*.rb").size} lib files)"
  out.each { |k, v, l| puts "  #{k}\t#{v}\t#{l}" }
end
