# A module holding only constants nests no definition: a constant holder,
# not a namespace this tree reopened to put its own code under, so a miss on
# it still accuses. CO-U drops the nested-definition check and must silence
# 9:8.
module Beacon
  INTERVAL = 5
end

Beacon.interval
