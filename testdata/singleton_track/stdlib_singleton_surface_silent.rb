# Family (e): stdlib class-object surfaces that live in no project index.
# Every one of these really answers under MRI, and every one of them was
# in the singleton NotFound residue before the inventory existed
# (FileUtils.* alone was 270 of discourse's explicit-receiver sites).
require "fileutils"
require "tmpdir"
require "securerandom"
require "digest"
require "json"

class Workspace
  def self.prepare(dir)
    FileUtils.mkdir_p(dir)
    FileUtils.cp(__FILE__, File.join(dir, "copy.rb"))
    FileUtils.rm_rf(dir)
    [SecureRandom.uuid, SecureRandom.hex(4), Digest::MD5.hexdigest("x"), JSON.parse("[1]")]
  end
end

out = Workspace.prepare(File.join(Dir.tmpdir, "itaruby-family-e-#{Process.pid}"))
raise "expected a uuid" unless out[0].length == 36
raise "expected math" unless Math.sqrt(4) == 2.0
raise "expected kernel" unless Kernel.rand(1) == 0
