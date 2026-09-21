# frozen_string_literal: true

# Bead ita-nst, ACCUSATION side: the nested `def self.fetch_data(x)` is
# FILED with its real arity, so a wrong-arity call on it is an E0102 the
# checker really reports — the proof the filing carries data, not just a
# name. MRI raises ArgumentError on the blamed line.
module Report
  def self.item(&block)
    block.call
  end

  def self.register!
    item do
      def self.fetch_data(x)
        x
      end
    end
  end
end

Report.register!
Report.fetch_data(1, 2)
