#!/usr/bin/env ruby
# frozen_string_literal: true
#
# Measurement-only census for bead ita-k9j.2: how many `included do` /
# `class_methods do` / `prepended do` / `extended do` blocks (ActiveSupport::
# Concern idiom) provably define NO method, vs. the total. Answers the design
# question behind `DefWalker::open_class(.., OpenReason::ClassBodyBlock)` in
# crates/itaruby_semantic/src/index.rs — it changes no product code.
#
# Needs Ruby >= 3.4 (prism ships with it). Measured 2026-08-22: the `work`
# machine has only system Ruby 2.6, where `require "prism"` fails — the
# corpus-a/corpus-b numbers in AGENTS.md therefore come from the Rust
# probe's call-site counts, not from this script.
#
# Usage:
#   ruby scripts/included-do-census.rb --self-test
#   ruby scripts/included-do-census.rb <root> [<root> ...] [--sites FILE]
#
# --sites FILE dumps "path:line" of every block classified `inert`, one per
# line. That output is CLIENT-IDENTIFYING (real file paths from the corpus
# under scan) and must only ever be written under a gitignored `target/`
# path — never committed, never printed to stdout, never pasted anywhere
# outside a local spot-check.
#
# The JSON on stdout is aggregate counts plus public Rails/framework call
# names only; root labels are basenames, never absolute paths.

require "prism"
require "set"
require "json"

# Transcribed verbatim from `fn defines_no_method` in
# crates/itaruby_semantic/src/index.rs (lines ~129-199). Every name here is
# documented public Rails/ActiveJob/Sidekiq API that registers a hook or
# validator and defines no method on the class body it runs in. Deliberately
# excludes every method generator (belongs_to, has_many, scope, enum, ...) —
# see METHOD_DEFINING below.
INERT_CALLS = Set.new(%w[
  validates validate validates_with validates_each validates_presence_of
  validates_uniqueness_of validates_length_of validates_format_of
  validates_numericality_of validates_inclusion_of validates_exclusion_of
  validates_associated validates_acceptance_of validates_confirmation_of
  validates_absence_of
  before_validation after_validation before_save after_save around_save
  before_create after_create around_create before_update after_update
  around_update before_destroy after_destroy around_destroy after_commit
  after_rollback after_initialize after_find after_touch
  before_action after_action around_action skip_before_action
  skip_after_action skip_around_action prepend_before_action
  prepend_after_action prepend_around_action rescue_from
  protect_from_forgery layout http_basic_authenticate_with
  helper helper_method
  queue_as retry_on discard_on sidekiq_options
  serialize attr_readonly default_scope
]).freeze

# Calls that provably (or plausibly enough to fail closed) define a method,
# per the assignment spec: metaprogramming primitives plus every AR/Rails
# association/attribute generator explicitly excluded from defines_no_method.
METHOD_DEFINING_EXPLICIT = %w[
  define_method define_singleton_method attr_accessor attr_reader attr_writer
  attr alias_method delegate delegate_missing_to class_eval module_eval
  instance_eval class_exec module_exec instance_exec send public_send
  __send__ eval include extend prepend const_set
].freeze
GENERATORS = %w[
  belongs_to has_many has_one has_and_belongs_to_many scope enum attribute
  store store_accessor alias_attribute composed_of
  accepts_nested_attributes_for has_secure_password has_secure_token
  has_one_attached has_many_attached monetize enumerize devise
].freeze
METHOD_DEFINING = Set.new(METHOD_DEFINING_EXPLICIT + GENERATORS).freeze
ACTS_AS_RE = /\Aacts_as_/

TARGET_NAMES = Set.new(%w[included class_methods prepended extended]).freeze

# Node kinds that can never define a method by themselves: structural
# containers (statements/arguments/block/array/hash wrappers — walking
# through them is required, not a classification decision) and literal/
# read leaves (symbol, string, numeric, boolean, nil, constant read, self,
# ivar/lvar read). Anything NOT in this list and not a CallNode/DefNode/
# AliasMethodNode is unrecognized structure (e.g. `if`, `case`, a lambda
# literal, a class reopen) and fails closed to `unknown`.
SAFE_NODE_TYPES = [
  Prism::StatementsNode, Prism::ArgumentsNode, Prism::BlockNode,
  Prism::ArrayNode, Prism::HashNode, Prism::KeywordHashNode,
  Prism::AssocNode, Prism::AssocSplatNode, Prism::SplatNode,
  Prism::ParenthesesNode,
  Prism::SymbolNode, Prism::StringNode, Prism::IntegerNode,
  Prism::FloatNode, Prism::RationalNode, Prism::ImaginaryNode,
  Prism::TrueNode, Prism::FalseNode, Prism::NilNode,
  Prism::ConstantReadNode, Prism::ConstantPathNode, Prism::SelfNode,
  Prism::InstanceVariableReadNode, Prism::LocalVariableReadNode,
].freeze

INERT = 0
UNKNOWN = 1
DEFINES_METHOD = 2
SEVERITY_NAME = { INERT => "inert", UNKNOWN => "unknown", DEFINES_METHOD => "defines_method" }.freeze

# Recursively classifies a found block's body. Returns
# [:inert|:unknown|:defines_method, per_call_histogram]. Most-conservative
# rule wins on conflict (defines_method > unknown > inert); every descendant
# is visited (deep, not shallow) so a `def` nested inside an `if` inside the
# block still flips the result.
def classify_block_body(body, hist)
  severity = INERT
  visit = lambda do |node|
    return if node.nil?
    case node
    when Prism::DefNode, Prism::AliasMethodNode
      severity = DEFINES_METHOD if severity < DEFINES_METHOD
      node.compact_child_nodes.each { |c| visit.call(c) }
    when Prism::CallNode
      name = node.name.to_s
      hist[:seen][name] += 1
      if METHOD_DEFINING.include?(name) || name.match?(ACTS_AS_RE)
        severity = DEFINES_METHOD if severity < DEFINES_METHOD
        hist[:method_defining][name] += 1
      elsif INERT_CALLS.include?(name)
        hist[:inert][name] += 1
      else
        severity = UNKNOWN if severity < UNKNOWN
        hist[:unrecognized][name] += 1
      end
      # A block on an inert call (e.g. `before_save { ... }`) cannot define a
      # method on the enclosing class by itself, but a `def` smuggled inside
      # it still would — recurse with the same rules regardless of bucket.
      visit.call(node.receiver)
      visit.call(node.arguments)
      visit.call(node.block)
    else
      unless SAFE_NODE_TYPES.any? { |k| node.is_a?(k) }
        severity = UNKNOWN if severity < UNKNOWN
      end
      node.compact_child_nodes.each { |c| visit.call(c) } if node.respond_to?(:compact_child_nodes)
    end
  end
  visit.call(body)
  SEVERITY_NAME[severity].to_sym
end

FoundBlock = Struct.new(:name, :container, :line, :classification)

# Walks a whole file's AST once, tracking lexical container (class body /
# module body / other) and collecting every `included`/`class_methods`/
# `prepended`/`extended do ... end` call found anywhere in the file.
def find_blocks(program_node, path)
  found = []
  visit = lambda do |node, container|
    return if node.nil?
    new_container =
      case node
      when Prism::ClassNode then :class
      when Prism::ModuleNode then :module
      when Prism::DefNode then :other
      else container
      end
    if node.is_a?(Prism::CallNode) && node.receiver.nil? &&
       TARGET_NAMES.include?(node.name.to_s) && node.block.is_a?(Prism::BlockNode)
      hist = { seen: Hash.new(0), inert: Hash.new(0), method_defining: Hash.new(0), unrecognized: Hash.new(0) }
      classification = classify_block_body(node.block.body, hist)
      found << [FoundBlock.new(node.name.to_s, container, node.location.start_line, classification), hist]
    end
    node.compact_child_nodes.each { |c| visit.call(c, new_container) }
  end
  visit.call(program_node, :other)
  found
end

def empty_root_stats
  {
    "files_scanned" => 0, "files_unreadable" => 0, "parse_errors" => 0,
    "blocks" => Hash.new(0),
    "lexical_parent" => Hash.new(0),
    "classification" => Hash.new(0),
    "classification_by_name" => Hash.new { |h, k| h[k] = Hash.new(0) },
    "inert_calls_seen" => Hash.new(0),
    "method_defining_calls_seen" => Hash.new(0),
    "unrecognized_calls_seen" => Hash.new(0),
  }
end

def merge_hist_into!(stats, hist)
  hist[:inert].each { |k, v| stats["inert_calls_seen"][k] += v }
  hist[:method_defining].each { |k, v| stats["method_defining_calls_seen"][k] += v }
  hist[:unrecognized].each { |k, v| stats["unrecognized_calls_seen"][k] += v }
end

# ponytail: symlinked directories are skipped rather than cycle-detected via
# realpath tracking — none of the three corpora in scope contain symlink
# loops; add realpath dedup if a future corpus does.
def scan_rb_files(root)
  files = []
  unreadable = 0
  stack = [root]
  until stack.empty?
    dir = stack.pop
    entries =
      begin
        Dir.children(dir)
      rescue SystemCallError
        unreadable += 1
        next
      end
    entries.each do |entry|
      path = File.join(dir, entry)
      if File.directory?(path)
        stack << path unless File.symlink?(path)
      elsif path.end_with?(".rb")
        if File.readable?(path)
          files << path
        else
          unreadable += 1
        end
      end
    end
  end
  [files, unreadable]
end

def census_root(root)
  stats = empty_root_stats
  files, unreadable_dirs = scan_rb_files(root)
  stats["files_unreadable"] += unreadable_dirs
  sites = [] # [path, line] for every inert block, filled by caller if wanted

  files.each do |path|
    stats["files_scanned"] += 1
    result =
      begin
        Prism.parse_file(path)
      rescue StandardError
        stats["files_unreadable"] += 1
        next
      end
    stats["parse_errors"] += 1 unless result.errors.empty?

    find_blocks(result.value, path).each do |block, hist|
      stats["blocks"][block.name] += 1
      stats["lexical_parent"][block.container.to_s] += 1
      stats["classification"][block.classification.to_s] += 1
      stats["classification_by_name"][block.name][block.classification.to_s] += 1
      merge_hist_into!(stats, hist)
      sites << [path, block.line] if block.classification == :inert
    end
  end

  [stats, sites]
end

def sort_desc(hash)
  hash.sort_by { |_, v| -v }.to_h
end

def finalize_stats!(stats)
  %w[included class_methods prepended extended].each { |n| stats["blocks"][n] = stats["blocks"][n] }
  %w[class module other].each { |c| stats["lexical_parent"][c] = stats["lexical_parent"][c] }
  %w[inert defines_method unknown].each { |c| stats["classification"][c] = stats["classification"][c] }
  by_name = stats["classification_by_name"]
  stats["classification_by_name"] = %w[included class_methods prepended extended].to_h do |n|
    by_class = by_name[n]
    %w[inert defines_method unknown].each { |c| by_class[c] = by_class[c] }
    [n, by_class]
  end
  stats["inert_calls_seen"] = sort_desc(stats["inert_calls_seen"])
  stats["method_defining_calls_seen"] = sort_desc(stats["method_defining_calls_seen"])
  stats["unrecognized_calls_seen"] = sort_desc(stats["unrecognized_calls_seen"])
  stats
end

def sum_into!(totals, stats)
  %w[files_scanned files_unreadable parse_errors].each { |k| totals[k] += stats[k] }
  stats["blocks"].each { |k, v| totals["blocks"][k] += v }
  stats["lexical_parent"].each { |k, v| totals["lexical_parent"][k] += v }
  stats["classification"].each { |k, v| totals["classification"][k] += v }
  stats["classification_by_name"].each do |name, by_class|
    by_class.each { |k, v| totals["classification_by_name"][name][k] += v }
  end
  stats["inert_calls_seen"].each { |k, v| totals["inert_calls_seen"][k] += v }
  stats["method_defining_calls_seen"].each { |k, v| totals["method_defining_calls_seen"][k] += v }
  stats["unrecognized_calls_seen"].each { |k, v| totals["unrecognized_calls_seen"][k] += v }
end

# ---------------------------------------------------------------------------
# --class-body-blocks mode (bead ita-k9j.2 entrega 1): measures the
# checker's OWN opening condition -- `Node::CallNode`'s block-check arm in
# `crates/itaruby_semantic/src/index.rs` (~lines 690-733): a call carrying
# an attached block, found directly in a class-or-module lexical body,
# opens that fragment as `OpenReason::ClassBodyBlock` -- except a bare
# `define_method(...) do ... end` (indexes the literal name, or opens
# under a DIFFERENT reason, `DynamicDefineMethod`, for a dynamic one -- so
# EITHER way it is never `ClassBodyBlock`) and a bare RECOGNIZED
# `sig { ... }` (`sig_block_is_recognized`). Below is a transcription of
# that arm plus the statement-level dispatch that reaches it
# (`walk_body`/`walk_stmts`/`walk_stmt`) checked line-by-line against the
# Rust source, not a guess from the doc comment.
# ---------------------------------------------------------------------------

OpeningCall = Struct.new(:name, :scope, :line, :classification)

# `join_path` transcribed from index.rs; Prism's own `full_name` already
# does the ConstantReadNode/ConstantPathNode walk `const_path_str`
# hand-rolls in Rust, so only the empty-scope/leading-`::` join rule needs
# repeating here.
def cbb_join_path(scope, name)
  if scope.empty? || name.start_with?("::")
    name.sub(/\A::/, "")
  else
    "#{scope}::#{name}"
  end
end

def cbb_scope_name(scope, constant_path_node)
  name =
    begin
      constant_path_node.full_name
    rescue StandardError
      nil
    end
  return nil if name.nil?
  cbb_join_path(scope, name)
end

# Transcribed from `sig_block_stmt` (index.rs): the block's single
# statement, or nil for no block, a multi-statement body, or an empty one.
def sig_block_stmt(call)
  block = call.block
  return nil unless block.is_a?(Prism::BlockNode)
  body = block.body
  return nil if body.nil?
  if body.is_a?(Prism::StatementsNode)
    stmts = body.body
    return nil if stmts.size != 1
    stmts.first
  else
    body
  end
end

# Transcribed from `sig_block_is_recognized` (index.rs): the exact shape
# `sig { ... }`'s open-class carve-out accepts -- outermost call `returns`
# with exactly one positional argument, or bare `void`. Anything else
# (multi-statement block, no block, any other outermost call) is
# unrecognized and still opens.
def sig_block_is_recognized(call)
  stmt = sig_block_stmt(call)
  return false unless stmt.is_a?(Prism::CallNode)
  case stmt.name.to_s
  when "returns"
    args = stmt.arguments
    !args.nil? && args.arguments.size == 1
  when "void"
    true
  else
    false
  end
end

# Walks one file's AST replicating `DefWalker::walk_body`/`walk_stmts`/
# `walk_stmt` (index.rs) closely enough to reproduce their ONE opening
# condition of interest -- every other index.rs concern (fragments, method
# indexing, ancestry, requires, mixins, ...) is dropped since none of it
# feeds this census. `scope` is the nearest enclosing class/module's
# lexical path (`""` at true file toplevel, matching `frag_idx == None`);
# a call under an empty scope never counts, mirroring
# `let Some(i) = frag_idx else { return };`.
def find_class_body_blocks(program_node, _path)
  found = []

  walk_stmt = nil
  walk_stmts = nil

  walk_body = lambda do |scope, node|
    return if node.nil?
    walk_stmts.call(scope, node)
  end

  walk_stmts = lambda do |scope, node|
    return if node.nil?
    case node
    when Prism::StatementsNode
      node.body.each { |stmt| walk_stmt.call(scope, stmt) }
    when Prism::BeginNode
      walk_stmts.call(scope, node.statements)
    when Prism::ProgramNode
      walk_stmts.call(scope, node.statements)
    else
      walk_stmt.call(scope, node)
    end
  end

  walk_stmt = lambda do |scope, node|
    return if node.nil?
    case node
    when Prism::ClassNode
      full = cbb_scope_name(scope, node.constant_path)
      walk_body.call(full, node.body) if full
    when Prism::ModuleNode
      full = cbb_scope_name(scope, node.constant_path)
      walk_body.call(full, node.body) if full
    when Prism::SingletonClassNode
      # Only `class << self` recurses, into the SAME enclosing scope
      # (index.rs reuses `frag_idx` there) -- a non-self singleton class
      # expression is never walked into, same as index.rs.
      walk_stmts.call(scope, node.body) if node.expression.is_a?(Prism::SelfNode)
    when Prism::DefNode
      # Deliberately a no-op: index.rs's `DefNode` arm never re-enters the
      # method body, so a block-taking call written inside a `def` cannot
      # reach the `CallNode` arm below at all -- verified against the
      # source, not assumed.
    when Prism::IfNode
      walk_stmts.call(scope, node.statements)
      walk_stmts.call(scope, node.consequent)
    when Prism::UnlessNode
      # index.rs's `Node::UnlessNode` arm walks ONLY `n.statements()` and
      # never touches an `else` clause -- a real asymmetry in the checker
      # (if/elsif/else all walk both arms; unless's else does not).
      # Replicated verbatim, not "fixed": this census measures the
      # checker as it exists.
      walk_stmts.call(scope, node.statements)
    when Prism::ElseNode
      walk_stmts.call(scope, node.statements)
    when Prism::CallNode
      block = node.block
      return unless block.is_a?(Prism::BlockNode)
      return if scope.empty?
      name = node.name.to_s
      receiver_none = node.receiver.nil?
      carved_out =
        (name == "define_method" && receiver_none) ||
        (name == "sig" && receiver_none && sig_block_is_recognized(node))
      return if carved_out
      hist = { seen: Hash.new(0), inert: Hash.new(0), method_defining: Hash.new(0), unrecognized: Hash.new(0) }
      classification = classify_block_body(block.body, hist)
      found << OpeningCall.new(name, scope, node.location.start_line, classification)
      # A call (with or without a block) consumes the whole statement in
      # index.rs -- no further recursion into receiver/arguments here.
    else
      # CaseNode, WhileNode, an inline `Class.new do ... end`, etc. --
      # index.rs's `_ => {}` catch-all: only if/unless/else get
      # conservative both-arm walking; nothing else is scanned.
    end
  end

  walk_body.call("", program_node.statements)
  found
end

def empty_cbb_root_stats
  {
    "files_scanned" => 0, "files_unreadable" => 0, "parse_errors" => 0,
    "opening_calls_total" => 0,
    "opening_call_names" => Hash.new(0),
    "classification" => Hash.new(0),
    "classification_by_name" => Hash.new { |h, k| h[k] = Hash.new(0) },
    "distinct_enclosing_scopes" => 0,
    "scopes_all_inert" => 0,
  }
end

def finalize_cbb_stats!(stats)
  %w[inert defines_method unknown].each { |c| stats["classification"][c] = stats["classification"][c] }
  stats["classification_by_name"].each_value do |h|
    %w[inert defines_method unknown].each { |c| h[c] = h[c] }
  end
  stats["opening_call_names"] = sort_desc(stats["opening_call_names"])
  stats
end

def sum_into_cbb!(totals, stats)
  %w[files_scanned files_unreadable parse_errors opening_calls_total
     distinct_enclosing_scopes scopes_all_inert].each { |k| totals[k] += stats[k] }
  stats["opening_call_names"].each { |k, v| totals["opening_call_names"][k] += v }
  stats["classification"].each { |k, v| totals["classification"][k] += v }
  stats["classification_by_name"].each do |name, by_class|
    by_class.each { |k, v| totals["classification_by_name"][name][k] += v }
  end
end

# `scope_class`'s keys are real lexical class/module PATHS harvested from
# the scanned corpus -- client-identifying. It is built, counted, and
# discarded entirely inside this method; only the two resulting INTEGERS
# (never a path) escape it.
def census_class_body_blocks_root(root)
  stats = empty_cbb_root_stats
  files, unreadable_dirs = scan_rb_files(root)
  stats["files_unreadable"] += unreadable_dirs
  sites = []
  scope_class = Hash.new { |h, k| h[k] = [] }

  files.each do |path|
    stats["files_scanned"] += 1
    result =
      begin
        Prism.parse_file(path)
      rescue StandardError
        stats["files_unreadable"] += 1
        next
      end
    stats["parse_errors"] += 1 unless result.errors.empty?

    find_class_body_blocks(result.value, path).each do |call|
      stats["opening_calls_total"] += 1
      stats["opening_call_names"][call.name] += 1
      stats["classification"][call.classification.to_s] += 1
      stats["classification_by_name"][call.name][call.classification.to_s] += 1
      scope_class[call.scope] << call.classification
      sites << [path, call.line] if call.classification == :inert
    end
  end

  stats["distinct_enclosing_scopes"] = scope_class.size
  stats["scopes_all_inert"] = scope_class.count { |_, arr| arr.all? { |c| c == :inert } }
  [stats, sites]
end

def run_class_body_blocks(roots, sites_file)
  totals = empty_cbb_root_stats
  root_reports = []
  all_sites = []

  roots.each do |root|
    stats, sites = census_class_body_blocks_root(root)
    finalize_cbb_stats!(stats)
    sum_into_cbb!(totals, stats)
    all_sites.concat(sites)
    root_reports << { "root" => File.basename(File.expand_path(root)) }.merge(stats)
  end

  finalize_cbb_stats!(totals)

  if sites_file
    File.open(sites_file, "w") do |f|
      all_sites.each { |path, line| f.puts("#{path}:#{line}") }
    end
  end

  puts JSON.generate({ "roots" => root_reports, "totals" => totals })
end

def run_self_test
  fixtures = {
    "before_save is inert" => ["included do\n before_save :x\nend", :inert],
    "def is method-defining" => ["included do\n def foo; end\nend", :defines_method],
    "belongs_to generator is method-defining" => ["included do\n belongs_to :y\nend", :defines_method],
    "scope generator is method-defining" => ["included do\n scope :recent, -> {}\nend", :defines_method],
    "validates with kwargs is inert" => ["included do\n validates :x, presence: true\nend", :inert],
    "unrecognized dsl is unknown" => ["included do\n some_unknown_dsl :x\nend", :unknown],
    "def nested in if is method-defining (deep walk)" => [
      "included do\n if Rails.env.test?\n  def foo; end\n end\nend", :defines_method,
    ],
    "def nested in inert call's block is method-defining" => [
      "included do\n before_save { def foo; end }\nend", :defines_method,
    ],
    "empty block is inert" => ["included do\nend", :inert],
    "class_methods with def is method-defining" => ["class_methods do\n def foo; end\nend", :defines_method],
  }

  failures = []
  fixtures.each do |fixture_name, (src, expected)|
    result = Prism.parse(src)
    found = find_blocks(result.value, "<fixture>")
    if found.empty?
      failures << [fixture_name, expected, "<no target block found>"]
      next
    end
    actual = found.first[0].classification
    failures << [fixture_name, expected, actual] unless actual == expected
  end

  cbb_fixtures = {
    "class-body-blocks: unrecognized dsl call with block counts, unknown" => [
      "class Foo\n  some_dsl do\n    another_unknown_thing\n  end\nend",
      [["some_dsl", :unknown]],
    ],
    "class-body-blocks: bare define_method(:literal) do..end never counts" => [
      "class Foo\n  define_method(:x) do\n  end\nend",
      [],
    ],
    "class-body-blocks: recognized sig { returns(X) } never counts" => [
      "class Foo\n  sig { returns(String) }\nend",
      [],
    ],
    "class-body-blocks: block-taking call nested inside a def never counts" => [
      "class Foo\n  def bar\n    some_dsl do\n    end\n  end\nend",
      [],
    ],
  }
  cbb_fixtures.each do |fixture_name, (src, expected)|
    result = Prism.parse(src)
    found = find_class_body_blocks(result.value, "<fixture>")
    actual = found.map { |call| [call.name, call.classification] }
    failures << [fixture_name, expected, actual] unless actual == expected
  end
  if failures.empty?
    puts "self-test ok"
    exit 0
  else
    failures.each do |name, expected, actual|
      warn "FAIL #{name}: expected=#{expected} actual=#{actual}"
    end
    exit 1
  end
end

def main
  args = ARGV.dup
  if args == ["--self-test"]
    run_self_test
    return
  end

  cbb_mode = false
  if (idx = args.index("--class-body-blocks"))
    args.delete_at(idx)
    cbb_mode = true
  end

  sites_file = nil
  if (idx = args.index("--sites"))
    args.delete_at(idx)
    sites_file = args.delete_at(idx)
  end

  roots = args
  if roots.empty?
    warn "usage: #{$PROGRAM_NAME} <root> [<root> ...] [--sites FILE]"
    warn "       #{$PROGRAM_NAME} --class-body-blocks <root> [<root> ...] [--sites FILE]"
    warn "       #{$PROGRAM_NAME} --self-test"
    exit 2
  end

  if cbb_mode
    run_class_body_blocks(roots, sites_file)
    return
  end

  totals = empty_root_stats
  root_reports = []
  all_sites = []

  roots.each do |root|
    stats, sites = census_root(root)
    finalize_stats!(stats)
    sum_into!(totals, stats)
    all_sites.concat(sites)
    root_reports << { "root" => File.basename(File.expand_path(root)) }.merge(stats)
  end

  finalize_stats!(totals)

  if sites_file
    File.open(sites_file, "w") do |f|
      all_sites.each { |path, line| f.puts("#{path}:#{line}") }
    end
  end

  puts JSON.generate({ "roots" => root_reports, "totals" => totals })
end

main if $PROGRAM_NAME == __FILE__
