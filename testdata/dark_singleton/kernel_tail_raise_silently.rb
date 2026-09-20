# frozen_string_literal: true

# The tail's headline name: a bare `raise` inside a `def self.` body. The
# class object really answers `raise` under MRI (Kernel's private instance
# surface reaches every object, class objects included), so the singleton
# `NotFound` residue on this name is a prospective FALSE E0101 — exactly
# what the tail inventory has to silence.
#
# MRI runs this file to completion.
class Importer
  def self.import(rows)
    raise ArgumentError, "no rows" if rows.empty?

    rows.size
  end
end

raise "expected two rows" unless Importer.import([1, 2]) == 2

begin
  Importer.import([])
  raise "expected the bare raise to fire"
rescue ArgumentError => e
  raise "unexpected message" unless e.message == "no rows"
end
