# frozen_string_literal: true

# Bead ita-src, CONTROL: the softening needs BOTH conditions. This class
# carries real methods of its own, so it is not a bare stub and keeps its
# whole surface checkable even though a string literal below writes a
# class of the same name. rails' `RaisesNoMethodError` fixture is the
# other half of the same control — a bare stub whose name NO string
# literal defines stays conclusive too.
class SrcNamed
  def self.src_real
    :real
  end
end

TEMPLATE = <<~RUBY
  class SrcNamed
    def self.src_generated
      :generated
    end
  end
RUBY

raise "bad" unless SrcNamed.src_real == :real
raise "bad" unless TEMPLATE.include?("src_generated")
