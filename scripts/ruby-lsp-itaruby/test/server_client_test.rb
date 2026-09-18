# frozen_string_literal: true

require_relative "test_helper"
require "fileutils"

module RubyLsp
  module Itaruby
    class ServerClientTest < Minitest::Test
      def test_happy_path_returns_items_with_real_range_and_multibyte_text
        Dir.mktmpdir do |dir|
          bin = FakeItaServer.write(dir)
          client = ServerClient.new(bin, dir, ->(text) { (@logs ||= []) << text })

          # Multibyte comment in the document text: if Content-Length were computed as a
          # character count instead of a byte count, framing would desync and this round trip
          # would fail (timeout or JSON parse error), not silently pass.
          items = client.diagnostics("file:///workspace/foo.rb", "# \u65e5\u672c\u8a9e comment\ndef foo; end\n")

          refute_nil items
          assert_equal 1, items.size
          item = items.first
          assert_equal 2, item[:range][:start][:line]
          assert_equal 4, item[:range][:start][:character]
          assert_equal 2, item[:range][:end][:line]
          assert_equal 9, item[:range][:end][:character]
          assert_equal "fake v1 \u65e5\u672c\u8a9e", item[:message]
        ensure
          client&.shutdown
        end
      end

      def test_second_call_sends_didchange_with_bumped_version
        Dir.mktmpdir do |dir|
          bin = FakeItaServer.write(dir)
          client = ServerClient.new(bin, dir)

          client.diagnostics("file:///workspace/foo.rb", "a")
          items = client.diagnostics("file:///workspace/foo.rb", "b")

          assert_equal "fake v2 \u65e5\u672c\u8a9e", items.first[:message]
        ensure
          client&.shutdown
        end
      end

      def test_dead_child_respawns_once_and_succeeds
        Dir.mktmpdir do |dir|
          marker = File.join(dir, "crashed-once")
          bin = FakeItaServer.write(dir)
          logs = []
          client = ServerClient.new(bin, dir, ->(text) { logs << text })

          with_env("ITARUBY_FAKE_MODE" => "crash_once", "ITARUBY_FAKE_MARKER" => marker) do
            items = client.diagnostics("file:///workspace/foo.rb", "a")

            refute_nil items
            assert_equal 1, items.size
            assert_equal "fake v1 \u65e5\u672c\u8a9e", items.first[:message]
          end

          assert(logs.any? { |m| m.include?("respawning") })
          refute client.disabled?
        ensure
          client&.shutdown
        end
      end

      def test_dying_twice_in_a_row_disables_the_client
        Dir.mktmpdir do |dir|
          bin = FakeItaServer.write(dir)
          logs = []
          client = ServerClient.new(bin, dir, ->(text) { logs << text })

          with_env("ITARUBY_FAKE_MODE" => "always_crash") do
            items = client.diagnostics("file:///workspace/foo.rb", "a")

            assert_nil items
          end

          assert client.disabled?
          assert(logs.any? { |m| m.include?("disabling") })

          # Disabled clients short-circuit without spawning another process.
          assert_nil client.diagnostics("file:///workspace/foo.rb", "a")
        ensure
          client&.shutdown
        end
      end

      # --- rootUri escaping (ita-dw8.7 fix 1) ---------------------------------------------

      def test_root_uri_percent_encodes_each_path_segment_but_not_the_slashes
        Dir.mktmpdir do |parent|
          # Space, accent, and "#" in one segment: any of the three breaks a naive
          # `"file://#{path}"` interpolation (space splits the URI, "#" starts a fragment,
          # and a non-ASCII byte is simply invalid in a URI).
          workspace = File.join(parent, "some dir", "caf\u00e9 #1")
          FileUtils.mkdir_p(workspace)
          marker = File.join(parent, "captured-root-uri")
          bin = write_fake_bin(parent, "capturing-ita", <<~RUBY)
            marker = #{marker.inspect}
            msg = read_message.call
            File.write(marker, msg.dig(:params, :rootUri).to_s)
            write_message.call(id: msg[:id], result: { capabilities: {} })
            sleep 60
          RUBY

          client = ServerClient.new(bin, workspace)
          client.send(:ensure_started!)

          captured = File.read(marker)

          assert captured.start_with?("file:///"), "expected an absolute file:// URI, got #{captured.inspect}"
          # "some dir" -> "some%20dir", "café #1" -> "caf%C3%A9%20%231" (each byte of "é",
          # the space, and "#" all separately percent-encoded); slashes stay literal.
          assert captured.end_with?("/some%20dir/caf%C3%A9%20%231"),
            "expected escaped path segments, got #{captured.inspect}"
          refute_includes captured, " "
          refute_includes captured, "\u00e9"
          refute_includes captured, "#1"
        ensure
          kill_wait_thread(client)
        end
      end

      # --- graceful shutdown handshake (ita-dw8.7 fix 2) ----------------------------------

      def test_shutdown_sends_lsp_shutdown_then_exit_before_the_process_exits
        Dir.mktmpdir do |dir|
          marker = File.join(dir, "sequence")
          bin = write_fake_bin(dir, "graceful-ita", <<~RUBY)
            marker = #{marker.inspect}
            log = ->(method) { File.write(marker, "\#{File.exist?(marker) ? File.read(marker) : ''}\#{method}\\n") }
            loop do
              msg = read_message.call
              break unless msg

              log.call(msg[:method])
              case msg[:method]
              when "initialize"
                write_message.call(id: msg[:id], result: { capabilities: {} })
              when "shutdown"
                write_message.call(id: msg[:id], result: nil)
              when "exit"
                exit!(0)
              end
            end
          RUBY

          client = ServerClient.new(bin, dir)
          client.send(:ensure_started!)

          client.shutdown

          assert_equal %w[initialize initialized shutdown exit], File.read(marker).split("\n")
          assert_nil client.instance_variable_get(:@wait_thread)
        end
      end

      def test_shutdown_tolerates_a_process_that_is_already_dead
        Dir.mktmpdir do |dir|
          bin = FakeItaServer.write(dir)
          client = ServerClient.new(bin, dir)
          client.send(:ensure_started!)

          pid = client.instance_variable_get(:@wait_thread).pid
          Process.kill("KILL", pid)
          sleep 0.1 # give the child a moment to actually die before we shut down "gracefully"

          # Must not raise Errno::ESRCH / Errno::EPIPE / IOError, even though the shutdown
          # request and exit notification are written to a process that's already gone.
          client.shutdown

          assert_nil client.instance_variable_get(:@wait_thread)
        end
      end

      # --- write timeout (ita-dw8.7 fix 3) -------------------------------------------------

      def test_write_that_never_completes_disables_the_client_instead_of_blocking_forever
        Dir.mktmpdir do |dir|
          bin = write_fake_bin(dir, "never-reading-ita", <<~RUBY)
            msg = read_message.call
            write_message.call(id: msg[:id], result: { capabilities: {} }) if msg
            sleep 60 # never read stdin again: the pipe fills up on the next big write
          RUBY
          logs = []
          client = ServerClient.new(bin, dir, ->(text) { logs << text }, write_timeout: 0.2)
          huge_text = "x" * 5_000_000 # bigger than any OS pipe buffer, forces a real block

          result = :never_ran
          thread = Thread.new { result = client.diagnostics("file:///workspace/foo.rb", huge_text) }
          finished = thread.join(10)
          thread.kill unless finished

          assert finished, "diagnostics blocked past the injected write_timeout instead of giving up"
          assert_nil result
          assert client.disabled?
          assert(logs.any? { |m| m.include?("disabling") })
        end
      end

      private

      # Shared LSP-over-stdio framing (identical wire format to FakeItaServer), with a custom
      # message loop `body` spliced in — lets each test spawn a minimal fake `<bin> server`
      # tailored to exactly the behavior it needs to observe, without touching test_helper.rb's
      # shared FakeItaServer.
      def write_fake_bin(dir, name, body)
        script = <<~RUBY
          # frozen_string_literal: true
          require "json"

          $stdin.binmode
          $stdout.binmode
          buffer = String.new(encoding: Encoding::ASCII_8BIT)

          read_message = lambda do
            loop do
              idx = buffer.index("\\r\\n\\r\\n")
              if idx
                header = buffer.slice!(0, idx + 4)
                length = header[/Content-Length:\\s*(\\d+)/i, 1].to_i
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
            resp = JSON.generate(payload.merge(jsonrpc: "2.0"))
            frame = +"Content-Length: \#{resp.bytesize}\\r\\n\\r\\n\#{resp}"
            $stdout.write(frame.b)
            $stdout.flush
          end

          #{body}
        RUBY

        path = File.join(dir, name)
        File.write(path, "#!#{RbConfig.ruby}\n#{script}")
        File.chmod(0o755, path)
        path
      end

      # For fixtures whose fake server never implements the shutdown/exit handshake (they only
      # need to prove something about the read/write side, not the shutdown side) — kills the
      # spawned process directly instead of going through `client.shutdown`'s handshake.
      def kill_wait_thread(client)
        return unless client

        wait_thread = client.instance_variable_get(:@wait_thread)
        return unless wait_thread

        begin
          Process.kill("KILL", wait_thread.pid)
        rescue Errno::ESRCH
          nil
        end
        wait_thread.join
      end
    end
  end
end
