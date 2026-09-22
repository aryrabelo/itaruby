# Bead ita-bgd (2026-09-21, dark-census measurement): `Kernel#BigDecimal`
# is installed by `require "bigdecimal"`, a BUNDLED gem in Ruby 3.4 —
# `ruby --disable-gems -e 'require "bigdecimal"'` raises LoadError, so
# `gen-core-inventory.rb`'s harvest can never see it. rails calls it with
# an implicit receiver inside a module body
# (`activesupport/test/json/encoding_test_cases.rb:78-79`), which the
# class-object track read as a conclusive miss.
require "bigdecimal"

module BgdEncodingTestCases
  NUMERIC_TESTS = [
    BigDecimal("0.0"),
    BigDecimal("2.5"),
  ].freeze
end
