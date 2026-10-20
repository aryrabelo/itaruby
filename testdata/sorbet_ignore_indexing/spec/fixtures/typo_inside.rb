# Checked, this would be an E0104 inside an ignored file.
class FixtureHolder
  def lookup
    UndefinedInsideIgnoredFixture
  end
end
