# An unrecognized class-body call stands the receiver down: singleton
# lookup returns Inconclusive, so this typo must stay silent and bucket
# open. CO-R removes that lookup guard and must emit E0101 at 14:8.
# The former emission-blocker mutant never reached its mutated arm here.
# closed_receiver_typo.rb is the same typo without the unknown DSL.
class Widget
  some_unknown_dsl :name

  def self.load_name
    "x"
  end
end

Widget.load_nmae
