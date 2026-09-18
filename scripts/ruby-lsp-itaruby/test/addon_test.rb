# frozen_string_literal: true

require_relative "test_helper"

module RubyLsp
  module Itaruby
    class DiagnosticRunnerTest < Minitest::Test
      def setup
        @queue = FakeMessageQueue.new
      end

      def test_run_diagnostic_maps_server_items_straight_into_lsp_diagnostics
        Dir.mktmpdir do |dir|
          bin = FakeItaServer.write(dir)
          runner = DiagnosticRunner.new(bin, dir, @queue)

          diagnostics = runner.run_diagnostic(FakeUri.new("#{dir}/foo.rb"), FakeDocument.new("def foo; end\n"))

          assert_equal 1, diagnostics.size
          first = diagnostics[0]
          assert_equal 2, first.range.start.line
          assert_equal 4, first.range.start.character
          assert_equal 2, first.range.end.line
          assert_equal 9, first.range.end.character
          assert_equal Constant::DiagnosticSeverity::ERROR, first.severity
          assert_equal "E0101", first.code
          assert_equal "itaruby", first.source
          assert_match(/fake v1/, first.message)
        end
      end

      def test_second_call_reuses_the_server_and_bumps_the_document_version
        Dir.mktmpdir do |dir|
          bin = FakeItaServer.write(dir)
          runner = DiagnosticRunner.new(bin, dir, @queue)
          uri = FakeUri.new("#{dir}/foo.rb")

          runner.run_diagnostic(uri, FakeDocument.new("a"))
          diagnostics = runner.run_diagnostic(uri, FakeDocument.new("b"))

          assert_match(/fake v2/, diagnostics.first.message)
        end
      end

      def test_missing_binary_ends_up_disabled_returns_empty_and_logs
        Dir.mktmpdir do |dir|
          missing_bin = File.join(dir, "definitely-not-a-real-binary")
          runner = DiagnosticRunner.new(missing_bin, dir, @queue)

          diagnostics = runner.run_diagnostic(FakeUri.new("#{dir}/foo.rb"), FakeDocument.new("def foo; end\n"))

          assert_equal [], diagnostics
          assert(@queue.messages.any? { |m| m.params[:message].include?("disabling") })
        end
      end

      def test_run_formatting_and_run_range_formatting_always_return_nil
        Dir.mktmpdir do |dir|
          runner = DiagnosticRunner.new("ita", dir, @queue)

          assert_nil runner.run_formatting(FakeUri.new("foo.rb"), nil)
          assert_nil runner.run_range_formatting(FakeUri.new("foo.rb"), "src", 0)
        end
      end
    end

    class AddonTest < Minitest::Test
      def test_addon_declares_a_ruby_lsp_version_constraint
        # depend_on_ruby_lsp! runs at addon.rb load time; deleting that call leaves nil here,
        # and changing it to a different range fails the assertion — both sides covered. The
        # constraint here MUST match the gemspec's add_dependency "ruby-lsp" range exactly.
        assert_equal [">= 0.23", "< 0.27"], RubyLsp::Addon.ruby_lsp_constraint
      end

      def test_activate_registers_formatter_and_survives_missing_binary
        addon = Addon.new
        global_state = FakeGlobalState.new("/tmp/fake-workspace")
        queue = FakeMessageQueue.new
        missing_bin = "/definitely/not/a/real/binary-#{Process.pid}"

        with_env("ITA_BIN" => missing_bin) do
          addon.activate(global_state, queue)
        end

        assert_kind_of DiagnosticRunner, global_state.formatters["itaruby"]
        assert_equal "itaruby", addon.name
        assert(queue.messages.any? { |m| m.params[:message].include?("not found") })
      end

      def test_activate_falls_back_to_addon_settings_bin_when_env_is_unset
        addon = Addon.new
        global_state = FakeGlobalState.new("/tmp/fake-workspace")
        global_state.addon_settings = { "itaruby" => { bin: "ita" } }
        queue = FakeMessageQueue.new

        with_env("ITA_BIN" => nil) do
          addon.activate(global_state, queue)
        end

        assert_kind_of DiagnosticRunner, global_state.formatters["itaruby"]
      end
    end
  end
end
