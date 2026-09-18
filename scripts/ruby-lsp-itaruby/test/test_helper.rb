# frozen_string_literal: true

$LOAD_PATH.unshift(File.expand_path("../lib", __dir__))

require "minitest/autorun"
require "tmpdir"
require "rbconfig"

# Minimal stand-ins for the slice of ruby-lsp our addon touches. Shapes checked against the real
# shopify/ruby-lsp gem v0.23.0 sources (lib/ruby_lsp/addon.rb, lib/ruby_lsp/utils.rb,
# lib/ruby_lsp/global_state.rb, lib/ruby_lsp/requests/support/formatter.rb) so a real interface
# drift shows up here too, without requiring the gem (and its network fetch) to run these tests.
module RubyLsp
  class Addon
    def activate(global_state, message_queue); end
    def deactivate; end
    def name; end
    def version; end
    # Real RubyLsp::Addon.depend_on_ruby_lsp! (ruby-lsp's addon.rb) takes a splat of version
    # constraints and validates them against the running ruby-lsp; the stand-in records them so
    # tests can assert our addon declared them. Checked against ruby-lsp 0.23.0 like the rest of
    # this file (real gem confirms the splat signature: `def depend_on_ruby_lsp!(*version_constraints)`).
    def self.depend_on_ruby_lsp!(*constraints)
      @ruby_lsp_constraint = constraints
    end

    def self.ruby_lsp_constraint
      @ruby_lsp_constraint
    end
  end

  module Requests
    module Support
      # Real interface: RubyLsp::Requests::Support::Formatter, three abstract methods.
      module Formatter
        def run_formatting(uri, document)
          raise NotImplementedError
        end

        def run_range_formatting(uri, source, base_indentation)
          raise NotImplementedError
        end

        def run_diagnostic(uri, document)
          raise NotImplementedError
        end
      end
    end
  end

  # Real RubyLsp aliases these to LanguageServer::Protocol::Interface/Constant (utils.rb).
  module Interface
    Position = Struct.new(:line, :character, keyword_init: true)
    Range = Struct.new(:start, :end, keyword_init: true)
    Diagnostic = Struct.new(:range, :message, :severity, :code, :source, keyword_init: true)
  end

  module Constant
    module MessageType
      ERROR = 1
      WARNING = 2
      INFO = 3
      LOG = 4
    end

    module DiagnosticSeverity
      ERROR = 1
      WARNING = 2
      INFORMATION = 3
      HINT = 4
    end
  end

  class Notification
    attr_reader :method, :params

    def initialize(method:, params:)
      @method = method
      @params = params
    end

    def self.window_log_message(message, type: Constant::MessageType::LOG)
      new(method: "window/logMessage", params: { type: type, message: message })
    end
  end
end

require "ruby_lsp/itaruby/server_client"
require "ruby_lsp/itaruby/diagnostic_runner"
require "ruby_lsp/itaruby/addon"

# Stand-in for RubyLsp::GlobalState#register_formatter/#workspace_path/#settings_for_addon,
# enough to test Addon#activate. Shapes checked against ruby-lsp 0.23.0's global_state.rb.
class FakeGlobalState
  attr_reader :formatters, :workspace_path

  def initialize(workspace_path = Dir.pwd)
    @formatters = {}
    @workspace_path = workspace_path
    @addon_settings = {}
  end

  def register_formatter(identifier, instance)
    @formatters[identifier] = instance
  end

  def settings_for_addon(name)
    @addon_settings[name]
  end

  attr_writer :addon_settings
end

# Stand-in for the Thread::Queue ruby-lsp passes as message_queue/outgoing_queue: records pushes.
class FakeMessageQueue
  attr_reader :messages

  def initialize
    @messages = []
  end

  def <<(message)
    @messages << message
  end
end

# Stand-in for the URI::Generic subclass ruby-lsp passes into run_diagnostic/run_formatting —
# real URI::Generic#to_s renders the "file:///..." form our ServerClient sends over the wire.
FakeUri = Struct.new(:path) do
  def to_standardized_path
    path
  end

  def to_s
    "file://#{path}"
  end
end

# Stand-in for RubyLsp::RubyDocument, which exposes the full current text as `#source`.
FakeDocument = Struct.new(:source)

# A real `<bin> server` child process, for tests: speaks LSP framing (Content-Length + JSON) over
# stdio well enough to exercise ServerClient/DiagnosticRunner end to end without the itaruby
# binary being built yet. Behavior is controlled by two env vars read once at startup so the
# *same* script can play "healthy server", "dies right after the handshake, once", and "always
# dies right after the handshake" — letting respawn-then-succeed and respawn-twice-then-disable
# be tested by spawning the identical fake binary under different ENV.
module FakeItaServer
  SCRIPT = <<~'RUBY'
    # frozen_string_literal: true
    require "json"

    $stdin.binmode
    $stdout.binmode
    buffer = String.new(encoding: Encoding::ASCII_8BIT)

    read_message = lambda do
      loop do
        idx = buffer.index("\r\n\r\n")
        if idx
          header = buffer.slice!(0, idx + 4)
          length = header[/Content-Length:\s*(\d+)/i, 1].to_i
          buffer << $stdin.readpartial(65_536) while buffer.bytesize < length
          body = buffer.slice!(0, length)
          break JSON.parse(body.force_encoding(Encoding::UTF_8), symbolize_names: true)
        end
        buffer << $stdin.readpartial(65_536)
      end
    rescue EOFError
      nil
    end

    write_message = lambda do |payload|
      body = JSON.generate(payload.merge(jsonrpc: "2.0"))
      frame = +"Content-Length: #{body.bytesize}\r\n\r\n#{body}"
      $stdout.write(frame.b)
      $stdout.flush
    end

    mode = ENV["ITARUBY_FAKE_MODE"]
    marker = ENV["ITARUBY_FAKE_MARKER"]
    versions = {}

    loop do
      msg = read_message.call
      break unless msg

      case msg[:method]
      when "initialize"
        write_message.call(id: msg[:id], result: { capabilities: {} })
      when "initialized"
        if mode == "crash_once" && marker && !File.exist?(marker)
          File.write(marker, "1")
          exit!(1)
        elsif mode == "always_crash"
          exit!(1)
        end
      when "textDocument/didOpen", "textDocument/didChange"
        uri = msg.dig(:params, :textDocument, :uri)
        versions[uri] = msg.dig(:params, :textDocument, :version)
      when "textDocument/diagnostic"
        uri = msg.dig(:params, :textDocument, :uri)
        write_message.call(
          id: msg[:id],
          result: {
            kind: "full",
            items: [
              {
                range: { start: { line: 2, character: 4 }, end: { line: 2, character: 9 } },
                severity: 1,
                code: "E0101",
                source: "itaruby",
                message: "fake v#{versions[uri]} \u65e5\u672c\u8a9e",
              },
            ],
          },
        )
      end
    end
  RUBY

  def self.write(dir, name: "fake-ita")
    path = File.join(dir, name)
    File.write(path, "#!#{RbConfig.ruby}\n#{SCRIPT}")
    File.chmod(0o755, path)
    path
  end
end

# Temporarily sets env vars for the duration of a block, restoring (or deleting) them after.
def with_env(vars)
  originals = vars.keys.to_h { |k| [k, ENV[k]] }
  vars.each { |k, v| ENV[k] = v }
  yield
ensure
  originals.each { |k, v| v.nil? ? ENV.delete(k) : ENV[k] = v }
end
