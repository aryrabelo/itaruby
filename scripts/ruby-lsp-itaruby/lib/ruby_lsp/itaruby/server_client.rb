# frozen_string_literal: true

require "json"
require "open3"
require "erb"

module RubyLsp
  module Itaruby
    # Talks LSP 3.17 over stdio to a single persistent `<bin> server` child process: raw
    # Content-Length framing, the initialize/initialized handshake, full-sync
    # didOpen/didChange, and textDocument/diagnostic pull requests. Owns the child's entire
    # lifecycle — one respawn is attempted after the pipe breaks or times out; a second
    # consecutive failure disables the client for the rest of the ruby-lsp session. Every
    # failure mode degrades to a logged message plus `nil`/`[]`, never an exception into the
    # host (ruby-lsp itself, or DiagnosticRunner on its behalf).
    class ServerClient
      READ_TIMEOUT_SECONDS = 10
      WRITE_TIMEOUT_SECONDS = 10
      SHUTDOWN_TIMEOUT_SECONDS = 2
      TERM_TIMEOUT_SECONDS = 1

      def initialize(bin, workspace_path, log_callback = nil, write_timeout: WRITE_TIMEOUT_SECONDS)
        @bin = bin
        @workspace_path = workspace_path
        @log_callback = log_callback
        @write_timeout = write_timeout
        @mutex = Mutex.new
        @next_id = 0
        @open_documents = {} # uri string -> last-sent version
        @disabled = false
        @respawn_attempted = false
        @stdin = nil
        @stdout = nil
        @wait_thread = nil
        @read_buffer = nil
      end

      def disabled?
        @disabled
      end

      # Returns the LSP `items` array (symbol-keyed diagnostic hashes) for `uri`/`text`, or
      # `nil` if the client is disabled or the round trip could not be recovered.
      def diagnostics(uri, text)
        return nil if @disabled

        @mutex.synchronize { perform_diagnostics(uri, text) }
      end

      def shutdown
        @mutex.synchronize { stop_process }
      end

      private

      def perform_diagnostics(uri, text)
        ensure_started!
        sync_document(uri, text)
        items = pull_diagnostics(uri)
        @respawn_attempted = false
        items
      rescue StandardError => e
        recover_from_failure(e, uri, text)
      end

      # ponytail: a fixed one-shot respawn, not a backoff/retry policy — the invariant we care
      # about is "never spin forever on a permanently broken binary", not maximum uptime. Upgrade
      # to bounded backoff if a flaky-but-recoverable server shows up in practice.
      def recover_from_failure(error, uri, text)
        stop_process

        if @respawn_attempted
          disable!(error)
          return nil
        end

        @respawn_attempted = true
        log("itaruby: server process failed (#{error.message}), respawning")

        begin
          ensure_started!
          sync_document(uri, text)
          items = pull_diagnostics(uri)
          @respawn_attempted = false
          items
        rescue StandardError => e
          stop_process
          disable!(e)
          nil
        end
      end

      def disable!(error)
        @disabled = true
        log("itaruby: server crashed twice in a row, disabling itaruby diagnostics for this " \
          "session (#{error.message})")
      end

      def ensure_started!
        return if @stdin

        start_process
        handshake
      end

      def start_process
        @stdin, @stdout, @wait_thread = Open3.popen2(@bin, "server")
        @stdin.binmode
        @stdout.binmode
        @read_buffer = String.new(encoding: Encoding::ASCII_8BIT)
      end

      def stop_process
        return unless @wait_thread

        pid = @wait_thread.pid
        request_graceful_exit
        wait_or_escalate(pid)
        cleanup_handles
      end

      # Sends the LSP shutdown request followed by the exit notification — the handshake a
      # well-behaved server uses to know it's being asked to leave, not just having its pipes
      # cut. Best-effort and unconditional: a process that already died, or a pipe that's
      # already broken or full (see `write_with_deadline`), must never turn into an exception
      # here — `stop_process` runs on every failure path, including ones where the child died
      # moments ago.
      def request_graceful_exit
        write_message(id: next_id, method: "shutdown")
        write_message(method: "exit")
      rescue StandardError
        nil
      end

      # A short window for the child to act on the handshake above and exit on its own —
      # `Thread#join` returns immediately if it already has. A server that ignores the
      # handshake (or never got it) gets TERM, then — if it survives that too — KILL.
      def wait_or_escalate(pid)
        return if @wait_thread.join(SHUTDOWN_TIMEOUT_SECONDS)

        kill_process(pid, "TERM")
        return if @wait_thread.join(TERM_TIMEOUT_SECONDS)

        kill_process(pid, "KILL")
        @wait_thread.join
      end

      # `Errno::ESRCH` here means the process already exited (and was reaped) between our last
      # check and this signal — not a bug, just a race stop_process must always tolerate.
      def kill_process(pid, signal)
        Process.kill(signal, pid)
      rescue Errno::ESRCH
        nil
      end

      def cleanup_handles
        begin
          @stdin&.close
        rescue IOError, Errno::EPIPE
          nil
        end

        begin
          @stdout&.close
        rescue IOError
          nil
        end

        @stdin = nil
        @stdout = nil
        @wait_thread = nil
        @read_buffer = nil
        @open_documents.clear
      end

      def handshake
        send_request(next_id, "initialize", {
          processId: Process.pid,
          rootUri: root_uri,
          capabilities: {},
        })
        send_notification("initialized", {})
      end

      # Percent-encodes each path segment (never the separating "/") so a workspace path with
      # a space, accent, or "#" still produces a valid `file://` URI instead of a silently
      # broken one the server ignores.
      def root_uri
        segments = @workspace_path.split("/", -1).map { |segment| ERB::Util.url_encode(segment) }
        "file://#{segments.join("/")}"
      end

      # First sight of a uri in this server's lifetime opens it (version 1); every later call
      # is a full-sync didChange with a bumped version. Cleared whenever the child restarts, so
      # a respawn always re-opens the uri it was asked about instead of assuming shared state.
      def sync_document(uri, text)
        version = @open_documents[uri]
        if version
          version += 1
          @open_documents[uri] = version
          send_notification("textDocument/didChange", {
            textDocument: { uri: uri, version: version },
            contentChanges: [{ text: text }],
          })
        else
          @open_documents[uri] = 1
          send_notification("textDocument/didOpen", {
            textDocument: { uri: uri, languageId: "ruby", version: 1, text: text },
          })
        end
      end

      def pull_diagnostics(uri)
        result = send_request(next_id, "textDocument/diagnostic", { textDocument: { uri: uri } })
        result[:items] || []
      end

      def next_id
        @next_id += 1
      end

      def send_request(id, method, params)
        write_message(id: id, method: method, params: params)
        await_response(id)
      end

      def send_notification(method, params)
        write_message(method: method, params: params)
      end

      def write_message(payload)
        body = JSON.generate(payload.merge(jsonrpc: "2.0"))
        # Content-Length is a byte count, not a character count — `.b` forces a byte-exact write
        # regardless of the string's or the pipe's declared encoding.
        frame = +"Content-Length: #{body.bytesize}\r\n\r\n#{body}"
        write_with_deadline(frame.b)
      end

      # Mirrors the read side (`wait_readable!`): a full pipe would otherwise block this write
      # inside @mutex, freezing every other ruby-lsp request (didChange et al.) behind it.
      # Writes whatever fits each time IO.select says the pipe is ready, until the whole frame
      # is out or @write_timeout passes.
      def write_with_deadline(bytes)
        deadline = Process.clock_gettime(Process::CLOCK_MONOTONIC) + @write_timeout
        offset = 0

        while offset < bytes.bytesize
          remaining = deadline - Process.clock_gettime(Process::CLOCK_MONOTONIC)
          raise "itaruby server timed out after #{@write_timeout}s writing a message" if remaining <= 0
          unless IO.select(nil, [@stdin], nil, remaining)
            raise "itaruby server timed out after #{@write_timeout}s writing a message"
          end

          written = @stdin.write_nonblock(bytes.byteslice(offset..-1), exception: false)
          offset += written unless written == :wait_writable
        end
      end

      # Blocks for our own request's response, discarding notifications that arrive in the
      # meantime (publishDiagnostics et al.) — except window/logMessage, which we forward to the
      # host for debugging.
      def await_response(id)
        loop do
          message = read_message
          raise "itaruby server closed the connection" unless message

          if message[:id] == id
            raise "itaruby server error: #{message.dig(:error, :message)}" if message[:error]

            return message[:result]
          elsif message[:method] == "window/logMessage"
            log("itaruby server: #{message.dig(:params, :message)}")
          end
        end
      end

      def read_message
        header = read_header
        return nil unless header

        length = header[/Content-Length:\s*(\d+)/i, 1]&.to_i
        return nil unless length&.positive?

        body = read_body(length)
        return nil unless body

        JSON.parse(body.force_encoding(Encoding::UTF_8), symbolize_names: true)
      end

      def read_header
        loop do
          idx = @read_buffer.index("\r\n\r\n")
          return @read_buffer.slice!(0, idx + 4) if idx

          chunk = read_more
          return nil unless chunk

          @read_buffer << chunk
        end
      end

      def read_body(length)
        while @read_buffer.bytesize < length
          chunk = read_more
          return nil unless chunk

          @read_buffer << chunk
        end
        @read_buffer.slice!(0, length)
      end

      def read_more
        wait_readable!
        @stdout.readpartial(65_536)
      rescue EOFError, Errno::EPIPE, IOError
        nil
      end

      def wait_readable!
        return if IO.select([@stdout], nil, nil, READ_TIMEOUT_SECONDS)

        raise "itaruby server timed out after #{READ_TIMEOUT_SECONDS}s waiting for a response"
      end

      def log(text)
        @log_callback&.call(text)
      rescue StandardError
        nil
      end
    end
  end
end
