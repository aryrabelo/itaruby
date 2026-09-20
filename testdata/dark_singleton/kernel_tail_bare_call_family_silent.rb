# frozen_string_literal: true

# The rest of the tail family, one bare call per `def self.` body: every
# name here is a Kernel method a class object really answers under MRI, and
# every one of them showed up in the singleton `NotFound` residue of the
# public corpora. None of them may bucket as residue once the inventory
# lands.
#
# MRI runs this file to completion.
require "uri"

class Probe
  def self.sample
    rand(1)
  end

  def self.trace
    caller(0)
  end

  def self.wrap(value)
    Array(value)
  end

  def self.endpoint(raw)
    URI(raw)
  end

  def self.announce(msg)
    puts(msg)
  end

  def self.pause
    sleep(0)
  end

  def self.given?
    block_given?
  end

  # Deliberately never called: the census reads this body statically, so
  # MRI never has to load the neighbouring fixture to exercise the
  # `require_relative` site.
  def self.boot_extras
    require_relative "kernel_tail_raise_silently"
  end
end

raise "rand" unless Probe.sample.zero?
raise "caller" unless Probe.trace.is_a?(Array)
raise "Array" unless Probe.wrap(nil) == []
raise "URI" unless Probe.endpoint("https://example.com/").host == "example.com"
raise "sleep" unless Probe.pause.zero?
raise "block_given?" if Probe.given?
raise "puts" unless Probe.announce(nil).nil?
