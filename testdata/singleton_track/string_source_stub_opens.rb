# frozen_string_literal: true

# Bead ita-src: the project WRITES Ruby source defining `SrcFoo` and loads
# it later, so the parsed tree's only `SrcFoo` is a bare stub that says
# nothing about the class the call really reaches. rails' isolation tests
# are exactly this shape (`railties/test/application/configuration_test.rb`
# `app_file "app/models/foo.rb", <<-RUBY class Foo < ApplicationRecord ...`
# followed by `Foo.attributes_for_inspect`).
class SrcFoo; end

GENERATED = <<~RUBY
  class SrcFoo
    def self.src_attributes_for_inspect
      [:foo]
    end
  end
RUBY

eval(GENERATED) # rubocop:disable Security/Eval

raise "bad" unless SrcFoo.src_attributes_for_inspect == [:foo]
