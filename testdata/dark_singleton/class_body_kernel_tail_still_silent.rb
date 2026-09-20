# frozen_string_literal: true

# The tail inside a CLASS BODY (not a `def self.` body): a bare call at
# class-body level also runs with the class object as `self`, so the same
# core-tail answer applies. MRI runs this file to completion.
class Boot
  puts "booting"

  def self.ok
    "ok"
  end
end

raise "expected Boot.ok" unless Boot.ok == "ok"
