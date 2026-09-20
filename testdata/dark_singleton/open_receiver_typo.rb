# An unrecognized class-body call stands the receiver down (the same
# default-deny the diagnostic track uses): the census must bucket this site
# open, never closed_notfound. Guards the bucketing against ignoring the
# blocker — the mutant that labels every NotFound closed would flip this
# fixture and fail the test.
class Widget
  some_unknown_dsl :name

  def self.load_name
    "x"
  end
end

Widget.load_nmae
