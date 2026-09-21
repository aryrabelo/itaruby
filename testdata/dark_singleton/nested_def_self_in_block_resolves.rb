# frozen_string_literal: true

# Bead ita-nst: `def self.fetch_data` inside a plain-yield block inside
# `def self.register!` (discourse's EmotionDashboardReport shape) defines on
# the module's singleton at runtime — the walker must file it, so the later
# `Report.fetch_data` call RESOLVES (records nothing in the census) instead
# of bucketing closed_notfound. MRI runs this file to completion.
module Report
  def self.item(&block)
    block.call
  end

  def self.register!(thing)
    thing.item do
      def self.fetch_data(x)
        x + 1
      end
    end
  end
end

Report.register!(Report)
raise "expected fetch_data to be defined" unless Report.fetch_data(1) == 2
