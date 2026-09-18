# frozen_string_literal: true

require "ruby_lsp_itaruby/version"
require "ruby_lsp/itaruby/diagnostic_runner"
# Declares the ruby-lsp version range this addon is compatible with: on mismatch ruby-lsp
# warns and skips activation instead of breaking server boot (add-ons guide, "Dependency
# constraints"; the API is experimental, so the guard is what makes drift visible).
# Keep in sync with the gemspec's `add_dependency "ruby-lsp"`.
RubyLsp::Addon.depend_on_ruby_lsp!(">= 0.23", "< 0.27")

module RubyLsp
  module Itaruby
    # Wires itaruby into ruby-lsp as a diagnostics formatter (`rubyLsp.linters: ["itaruby"]`).
    # Discovered by ruby-lsp via `Gem.find_files("ruby_lsp/**/addon.rb")` — this file's path
    # (lib/ruby_lsp/itaruby/addon.rb) is what makes that discovery work; do not move it.
    class Addon < RubyLsp::Addon
      def activate(global_state, message_queue)
        bin = resolve_bin(global_state)

        unless binary_resolvable?(bin)
          log(
            message_queue,
            "itaruby: binary '#{bin}' not found (checked ITA_BIN, addonSettings.bin and PATH). " \
              "Diagnostics will stay silent until it is installed.",
          )
        end

        global_state.register_formatter(
          "itaruby",
          DiagnosticRunner.new(bin, global_state.workspace_path, message_queue),
        )
      end

      def deactivate; end

      def name
        "itaruby"
      end

      def version
        RubyLspItaruby::VERSION
      end

      private

      # ITA_BIN wins when set (documented, explicit escape hatch); otherwise an editor can pin
      # a binary per-workspace via initializationOptions.addonSettings.itaruby.bin.
      def resolve_bin(global_state)
        return ENV["ITA_BIN"] if ENV["ITA_BIN"]

        settings = global_state.respond_to?(:settings_for_addon) ? global_state.settings_for_addon("itaruby") : nil
        settings&.dig(:bin) || "ita"
      end

      # Best-effort existence check only — never raises. The runner itself is defensive against
      # the binary vanishing or failing between this check and an actual `run_diagnostic` call.
      def binary_resolvable?(bin)
        return true if bin.include?(File::SEPARATOR) && File.executable?(bin) && !File.directory?(bin)

        ENV.fetch("PATH", "").split(File::PATH_SEPARATOR).any? do |dir|
          candidate = File.join(dir, bin)
          File.executable?(candidate) && !File.directory?(candidate)
        end
      rescue StandardError
        false
      end

      def log(message_queue, text)
        return unless message_queue

        message_queue << RubyLsp::Notification.window_log_message(
          text,
          type: RubyLsp::Constant::MessageType::WARNING,
        )
      rescue StandardError
        nil
      end
    end
  end
end
