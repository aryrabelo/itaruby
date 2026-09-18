#!/usr/bin/env ruby
# frozen_string_literal: true
#
# scripts/navfixture/workload.rb — toy Ruby app, stdlib only (no Gemfile, no
# gem), that exists to give gate (e) (scripts/nav-oracle.rb +
# scripts/nav-gate.sh) a ground-truth fixture every machine actually has,
# instead of needing a bootable client Rails app. Each construct below is
# exercised by a real call site further down so the tracer can harvest a
# genuine Object#method(:x).source_location for it — this file IS the
# oracle's input, not a description of one. Lives under scripts/, not at
# the repo root, so it needs no `sf check` root-file declaration.
#
# Run standalone: `ruby scripts/navfixture/workload.rb` (exit 0 on success).
#
# Regenerate scripts/navfixture/oracle.jsonl after editing this file:
#   ruby scripts/nav-oracle.rb scripts/navfixture workload.rb \
#     --out scripts/navfixture/oracle.jsonl --relative

# 1. instance method defined on its own class
class Animal
  def speak
    '...'
  end
end

# 2/3. inheritance: one level (Parent) and two levels (Grandparent)
class Grandparent
  def gp_method
    'gp'
  end
end

class Parent < Grandparent
  def parent_method
    'parent'
  end
end

class Child < Parent
end

# 4. method from an included module
module Greetable
  def greet
    'hi from module'
  end
end

class Greeter
  include Greetable
end

# 5/6. prepend wins over the class's own method; the module's `super` call
# reaches back to the class's own definition — two distinct call sites in
# one exercise, the case that separates a correct MRO from a wrong one.
class Announcer
  def greet
    'hi'
  end
end

module LoudAnnounce
  def greet
    "LOUD: #{super}"
  end
end

class Announcer
  prepend LoudAnnounce
end

# 7. module extend (class-level method)
module ClassHelper
  def factory
    new
  end
end

class Widget
  extend ClassHelper
end

# 8. singleton method, `def self.x` form
class Config
  def self.default
    new
  end
end

# 9. singleton method, `class << self` form
class Toggle
  class << self
    def flip
      'flipped'
    end
  end
end

# 10. class reopened later in the same file — the second definition is the
# one that must win, both at runtime and in `ita definition`.
class Reopened
  def value
    'first'
  end
end

class Reopened
  def value
    'second'
  end
end

# 11. call without a receiver inside a method body (implicit self)
class SelfCaller
  def helper
    'helped'
  end

  def caller_method
    helper
  end
end

# 12. dynamically defined method (define_method)
class Dynamic
  define_method(:generated_greeting) do
    'dynamic hi'
  end
end

# 13. must stay silent: an open class (method_missing present) receiving a
# call to a method that has no real `def`. `ita definition` on this call
# site has nothing correct to point at — silence (unknown) is the only
# right answer; a confident guess would violate invariant #1.
class OpenGuess
  def method_missing(name, *_args)
    "guessed #{name}"
  end

  def respond_to_missing?(_name, _include_private = false)
    true
  end
end

# 14. a plain top-level method (no enclosing class)
def top_level_method
  'top'
end

# --- workload: call every construct above so the tracer records a real
# call site for each, in the same order as the definitions.

animal = Animal.new
raise 'fail: Animal#speak' unless animal.speak == '...'

child = Child.new
raise 'fail: Parent#parent_method' unless child.parent_method == 'parent'
raise 'fail: Grandparent#gp_method' unless child.gp_method == 'gp'

raise 'fail: Greetable#greet' unless Greeter.new.greet == 'hi from module'

announcer = Announcer.new
raise 'fail: LoudAnnounce#greet (prepend)' unless announcer.greet == 'LOUD: hi'

raise 'fail: ClassHelper#factory (extend)' unless Widget.factory.is_a?(Widget)

raise 'fail: Config self-method (def self.x)' unless Config.default.is_a?(Config)
raise 'fail: Toggle singleton (class << self)' unless Toggle.flip == 'flipped'

raise 'fail: Reopened#value (second def)' unless Reopened.new.value == 'second'

raise 'fail: SelfCaller#caller_method (implicit self)' unless SelfCaller.new.caller_method == 'helped'

raise 'fail: Dynamic#generated_greeting (define_method)' unless Dynamic.new.generated_greeting == 'dynamic hi'

open_guess = OpenGuess.new
mystery = open_guess.mystery_method
raise 'fail: OpenGuess#method_missing' unless mystery == 'guessed mystery_method'

raise 'fail: top-level def' unless top_level_method == 'top'

warn 'scripts/navfixture/workload.rb: ok'
