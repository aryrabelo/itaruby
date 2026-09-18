class Widget2
  def self.build
    new
  end

  def spin
    helper
  end

  def helper
    "spinning"
  end
end

Widget2.build
Widget2.new.spin
