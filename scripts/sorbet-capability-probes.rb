#!/usr/bin/env ruby
# frozen_string_literal: true

# The README's "Support matrix" (section "Sorbet vs itaruby") as an exit
# code. Every row there that is not marked *(docs)* is a probe below: one
# minimal program, the error codes `srb tc` must report on it, and the
# codes `ita check` must report on it. A probe whose observed codes differ
# from its expectation is a row of the README that is no longer true.
#
# Why it exists (measured 2026-10-22): the first draft of the matrix was
# written from memory and four Sorbet cells were wrong — refinements
# (Sorbet false-positives, it does not support them), inline RBS
# (experimental, behind two flags), the LSP feature list and the
# `typed: false` noise example — plus one itaruby cell (E0103 skips a
# top-level `def`). Each is now a probe.
#
# Run it when `srb` changes version, when a contracts PR lands, or when the
# matrix is edited. It is an instrument, not a gate: nothing calls it from
# `gauntlet-gates.sh`. Its own two-sided proof is
# `scripts/sorbet-capability-probes-selftest.sh`.
#
#   ruby scripts/sorbet-capability-probes.rb [--ita PATH] [--srb PATH] [--only ID]
#
#   0  every probe matched
#   1  a probe diverged (named), or the ita binary could not be built
#   2  SKIP: no `srb`, or not the pinned version — nothing was judged
#
# Without --ita it builds `target/release/ita` first and refuses to judge a
# binary older than any source under crates/ (AGENTS.md: evidence proves
# the binary, not the code).

require 'fileutils'
require 'json'
require 'open3'
require 'optparse'

SRB_VERSION = '0.6.13437'
ROOT = File.expand_path('..', __dir__)
WORK = File.join(ROOT, 'target', 'sorbet-capability-probes')

HOST = <<~RUBY
  class A
    def x; end
    def m(a); end
    def k(a:); end
  end
RUBY

RBS_FLAGS = %w[--enable-experimental-rbs-comments --parser=prism].freeze

# id, sigil, source, srb codes, ita codes, optional srb flags.
# [] means "silent". Codes are compared as sorted unique sets.
PROBES = [
  ['undef_instance', 'true', "#{HOST}A.new.y\n", %w[7003], %w[E0101]],
  ['undef_class_object', 'true', "#{HOST}A.zz\n", %w[7003], %w[E0101]],
  ['positional_arity', 'true', "#{HOST}A.new.m(1, 2)\n", %w[7004], %w[E0102]],
  ['keyword_arguments', 'true', "#{HOST}A.new.k(b: 1)\n", %w[7004], []],
  ['unresolved_constant_typed_false', 'false', "Foo::Bar\n", %w[5002], %w[E0104]],
  ['operator_on_literals', 'true', "1 + \"s\"\n", %w[7002], %w[E0108]],
  ['unannotated_body_typed_false', 'false', <<~RUBY, [], %w[E0101 E0102]],
    #{HOST}
    def f
      A.new.y
      A.new.m(1, 2)
    end
  RUBY
  ['sig_params', 'true', <<~RUBY, %w[7002], []],
    extend T::Sig
    sig { params(a: Integer).void }
    def p(a); end
    p("s")
  RUBY
  ['sig_returns', 'true', <<~RUBY, %w[7005], []],
    extend T::Sig
    sig { returns(Integer) }
    def r; "s"; end
  RUBY
  ['inline_rbs_default_flags', 'true', <<~RUBY, [], %w[E0103]],
    class D
      #: (Integer) -> void
      def q(a); end
    end
    D.new.q("s")
  RUBY
  ['inline_rbs_project_class', 'true', <<~RUBY, %w[7002], %w[E0103], RBS_FLAGS],
    class D
      #: (Integer) -> void
      def q(a); end
    end
    D.new.q("s")
  RUBY
  ['inline_rbs_toplevel_def', 'true', <<~RUBY, %w[7002], [], RBS_FLAGS],
    #: (Integer) -> void
    def q(a); end
    q("s")
  RUBY
  ['nilable_return', 'true', <<~RUBY, %w[7003], []],
    extend T::Sig
    sig { returns(T.nilable(String)) }
    def n; end
    n.length
  RUBY
  ['generic_element', 'true', <<~RUBY, %w[7003], []],
    extend T::Sig
    sig { returns(T::Array[Integer]) }
    def g; [1]; end
    g.first.upcase
  RUBY
  ['t_let', 'true', "x = T.let(\"s\", Integer)\n", %w[7007], []],
  ['t_struct', 'true', <<~RUBY, %w[7002], []],
    class S < T::Struct
      const :a, Integer
    end
    S.new(a: "s")
  RUBY
  ['typed_strict_demands_sigs', 'strict', "class B\n  def nosig; end\nend\n", %w[7017], []],
  ['ivar_narrowing', 'true', <<~RUBY, [], []],
    class C
      extend T::Sig
      sig { void }
      def initialize; @x = T.let(nil, T.nilable(String)); end
      sig { void }
      def f; if @x; @x.length; end; end
    end
  RUBY
  ['ivar_narrowing_control', 'true', <<~RUBY, %w[7003], []],
    class C
      extend T::Sig
      sig { void }
      def initialize; @x = T.let(nil, T.nilable(String)); end
      sig { void }
      def f; @x.length; end
    end
  RUBY
  ['block_parameter_type', 'true', "[1, 2].each { |x| x.upcase }\n", %w[7003], []],
  ['refinement', 'true', <<~RUBY, %w[7003], []],
    module R
      refine String do
        def zz; end
      end
    end
    using R
    "a".zz
  RUBY
  ['unshimmed_gem_constant_typed_false', 'false', "Nokogiri::HTML.parse(\"\")\n", %w[5002], []]
].freeze

# Directory probes: both tools run with the probe dir as cwd and read its
# sorbet/config. The control is the same tree without the config, so the
# ignored file is proven to be one both tools WOULD report.
IGNORED = { 'bad/x.rb' => "# typed: true\n1 + \"s\"\n", 'ok.rb' => "# typed: true\n1 + 1\n" }.freeze
DIR_PROBES = [
  ['sorbet_config_ignore', IGNORED.merge('sorbet/config' => ".\n--ignore=/bad/\n"), [], []],
  ['sorbet_config_ignore_control', IGNORED.merge('sorbet/config' => ".\n"), %w[7002], %w[E0108]]
].freeze

LSP_PROBES = {
  'srb' => %w[codeActionProvider completionProvider definitionProvider documentSymbolProvider
              hoverProvider implementationProvider referencesProvider renameProvider
              sorbetShowSymbolProvider textDocumentSync typeDefinitionProvider workspaceSymbolProvider],
  'ita' => %w[definitionProvider diagnosticProvider hoverProvider textDocumentSync]
}.freeze

def parse_args
  opts = { ita: nil, srb: 'srb', only: nil }
  OptionParser.new do |o|
    o.on('--ita PATH') { |v| opts[:ita] = v }
    o.on('--srb PATH') { |v| opts[:srb] = v }
    o.on('--only ID') { |v| opts[:only] = v }
  end.parse!
  opts
end

def build_ita
  ita = File.join(ROOT, 'target', 'release', 'ita')
  _out, status = Open3.capture2e('cargo', 'build', '--release', '--locked', '--target-dir', File.join(ROOT, 'target'),
                                 chdir: ROOT)
  abort_with(1, 'FAIL cargo build — refusing to judge a stale binary') unless status.success?
  stale = Dir.glob(File.join(ROOT, 'crates', '**', '*')).find { |f| File.file?(f) && File.mtime(f) > File.mtime(ita) }
  abort_with(1, "FAIL binary is older than #{stale}") if stale
  ita
end

def abort_with(code, message)
  puts message
  exit code
end

def check_srb(srb)
  out, status = Open3.capture2e(srb, '--version')
  abort_with(2, "SKIP no runnable `#{srb}` — nothing judged") unless status.success?
  return if out.include?("Sorbet typechecker #{SRB_VERSION} ")

  abort_with(2, "SKIP srb is not #{SRB_VERSION} (#{out.lines.first&.strip}) — re-measure the matrix, then re-pin")
rescue Errno::ENOENT
  abort_with(2, "SKIP no `#{srb}` on PATH — nothing judged")
end

def srb_codes(output)
  output.scan(%r{https://srb\.help/(\d+)}).flatten.uniq.sort
end

def ita_codes(output)
  output.scan(/(?:error|warning)\[(E\d+)\]/).flatten.uniq.sort
end

def write_tree(dir, files)
  FileUtils.rm_rf(dir)
  files.each do |rel, body|
    path = File.join(dir, rel)
    FileUtils.mkdir_p(File.dirname(path))
    File.write(path, body)
  end
end

def run_file_probe(tools, id, sigil, src, flags)
  dir = File.join(WORK, id)
  write_tree(dir, 't.rb' => "# typed: #{sigil}\n#{src}")
  srb_out, = Open3.capture2e(tools[:srb], 'tc', '--no-config', '--dir', dir, *flags)
  ita_out, = Open3.capture2e(tools[:ita], 'check', File.join(dir, 't.rb'))
  [srb_codes(srb_out), ita_codes(ita_out)]
end

def run_dir_probe(tools, id, files)
  dir = File.join(WORK, id)
  write_tree(dir, files)
  srb_out, = Open3.capture2e(tools[:srb], 'tc', chdir: dir)
  ita_out, = Open3.capture2e(tools[:ita], 'check', '.', chdir: dir)
  [srb_codes(srb_out), ita_codes(ita_out)]
end

def lsp_capabilities(cmd, dir)
  Open3.popen3(*cmd, chdir: dir) do |stdin, stdout, _stderr, wait|
    msg = JSON.generate(jsonrpc: '2.0', id: 1, method: 'initialize',
                        params: { processId: nil, rootUri: "file://#{dir}", capabilities: {} })
    begin
      stdin.write("Content-Length: #{msg.bytesize}\r\n\r\n#{msg}")
      stdin.flush
    rescue Errno::EPIPE
      nil # the child already exited; the empty capability set is the verdict
    end
    length = nil
    while (line = stdout.gets) && line != "\r\n"
      length = line.split(':').last.to_i if line.start_with?('Content-Length')
    end
    caps = length ? JSON.parse(stdout.read(length)).dig('result', 'capabilities') || {} : {}
    begin
      Process.kill('KILL', wait.pid)
    rescue Errno::ESRCH
      nil # the child already exited: a server that never answered
    end
    caps.select { |_, v| v }.keys.sort
  end
end

def verdict(id, want, got)
  ok = want == got
  puts format('%-4s %-36s srb=%-14s ita=%s', ok ? 'PASS' : 'FAIL', id, show(got[0]), show(got[1]))
  puts format('     %-36s expected srb=%-5s ita=%s', '', show(want[0]), show(want[1])) unless ok
  ok
end

def show(codes)
  codes.is_a?(Array) && codes.empty? ? 'silent' : Array(codes).join(',')
end

def main
  opts = parse_args
  check_srb(opts[:srb])
  tools = { srb: opts[:srb], ita: opts[:ita] || build_ita }
  puts "srb #{SRB_VERSION} vs #{tools[:ita]}"
  results = []
  PROBES.each do |id, sigil, src, srb, ita, flags|
    next if opts[:only] && opts[:only] != id

    results << verdict(id, [srb, ita], run_file_probe(tools, id, sigil, src, flags || []))
  end
  DIR_PROBES.each do |id, files, srb, ita|
    next if opts[:only] && opts[:only] != id

    results << verdict(id, [srb, ita], run_dir_probe(tools, id, files))
  end
  unless opts[:only] && opts[:only] != 'lsp_capabilities'
    dir = File.join(WORK, 'lsp')
    write_tree(dir, 'a.rb' => "# typed: true\nclass A; end\n")
    got = [lsp_capabilities([tools[:srb], 'tc', '--lsp', '--no-config', '--disable-watchman', '--dir', dir], dir),
           lsp_capabilities([tools[:ita], 'server'], dir)]
    results << verdict('lsp_capabilities', [LSP_PROBES['srb'], LSP_PROBES['ita']], got)
  end
  abort_with(1, "FAIL --only #{opts[:only]} matched no probe") if results.empty?
  failed = results.count(false)
  abort_with(1, "RESULT: FAIL (#{failed} of #{results.size} probes diverged — the README matrix is stale)") if failed.positive?
  puts "RESULT: PASS (#{results.size} probes)"
end

main
