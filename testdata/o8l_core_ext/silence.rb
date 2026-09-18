# Bead ita-o8l.2 / ita-o8l.3 -- ActiveSupport's core_ext family
# (`to_json`, `try`, `instance_values`, `acts_like?`) reopens `Object`,
# and RubyGems adds `Kernel#gem`; neither ActiveSupport nor RubyGems is
# loaded when `gen-core-inventory.rb` harvests the core method table
# (activesupport's own `.rbs` reopens `Object`, which is exactly the
# shape `core_stdlib_tops` filters OUT of the generated pack, per wave 9
# defect 1; `gen-core-inventory.rb` runs `ruby --disable-gems`). Real
# corpus sites: actionpack/.../journey/gtg/transition_table.rb:143
# (`to_json`), activemodel/.../error_test.rb:31 (`try`),
# activemodel/.../serialization_test.rb:40 (`instance_values`),
# activesupport/.../acts_like_test.rb:45 (`acts_like?`),
# actionpack/.../system_testing/driver.rb:18 (`gem`). None of these five
# names may ever accuse E0101 on any receiver again -- see `control.rb`
# for the allowlist-narrowness counter-proof.

class O8lCoreExtWidget
  # to_json: bare self-send, interpolated (transition_table.rb:143 shape).
  def to_transition_json
    "function tt() { return #{to_json}; }"
  end

  # try: bare self-send (error_test.rb:31 shape).
  def read_attribute_for_validation(attr)
    try(attr)
  end

  # instance_values: bare self-send (serialization_test.rb:40 shape).
  def attributes
    instance_values
  end

  # gem: bare Kernel self-send (driver.rb:18 shape).
  def load_selenium_driver
    gem "selenium-webdriver", ">= 4.0.0"
  end
end

class O8lCoreExtProbe
  # acts_like?: explicit receiver on a resolved, closed project class
  # whose own ancestry falls back to Object (acts_like_test.rb shape).
  def time_like?
    O8lCoreExtWidget.new.acts_like?(:time)
  end
end
