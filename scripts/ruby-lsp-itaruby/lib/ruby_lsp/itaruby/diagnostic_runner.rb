# frozen_string_literal: true

require "ruby_lsp/itaruby/server_client"

module RubyLsp
  module Itaruby
    # Bridges ruby-lsp's Formatter#run_diagnostic to a persistent `ita server` process speaking
    # standard LSP 3.17 pull diagnostics (textDocument/diagnostic). One ServerClient lives for
    # the lifetime of this runner and lazily spawns its child on the first call; the client owns
    # process lifecycle, respawn-once-then-disable, and wire framing. This class only adapts the
    # ruby-lsp interface to that client and maps LSP diagnostic items straight into
    # RubyLsp::Interface::Diagnostic (ranges come from the server already 0-based, no
    # conversion needed).
    class DiagnosticRunner
      include RubyLsp::Requests::Support::Formatter

      def initialize(bin, workspace_path, message_queue = nil)
        @message_queue = message_queue
        @client = ServerClient.new(bin, workspace_path, method(:log))
      end

      # We never format — itaruby only reports diagnostics.
      def run_formatting(uri, document)
        nil
      end

      def run_range_formatting(uri, source, base_indentation)
        nil
      end

      def run_diagnostic(uri, document)
        items = @client.diagnostics(uri.to_s, document.source)
        return [] unless items

        items.map { |item| to_diagnostic(item) }
      rescue StandardError => e
        log("itaruby: unexpected error running diagnostics: #{e.message}")
        []
      end

      private

      def to_diagnostic(item)
        range = item[:range] || {}
        start_pos = position_from(range[:start])
        end_pos = range[:end] ? position_from(range[:end]) : start_pos

        Interface::Diagnostic.new(
          range: Interface::Range.new(start: start_pos, end: end_pos),
          message: item[:message].to_s,
          severity: item[:severity],
          code: item[:code],
          source: item[:source] || "itaruby",
        )
      end

      def position_from(pos)
        return Interface::Position.new(line: 0, character: 0) unless pos

        Interface::Position.new(line: pos[:line].to_i, character: pos[:character].to_i)
      end

      def log(text)
        return unless @message_queue

        @message_queue << RubyLsp::Notification.window_log_message(
          text,
          type: RubyLsp::Constant::MessageType::WARNING,
        )
      rescue StandardError
        nil
      end
    end
  end
end
