# frozen_string_literal: true
#
# Precedence fixture (bead ita-muf): deliberately conflicts with the
# sibling db/structure.sql below on the same column's type, so whichever
# source wins is directly observable in `ita check` output.

ActiveRecord::Schema[7.1].define(version: 2026_08_20_000000) do
  create_table "widgets", force: :cascade do |t|
    t.integer "quantity"
  end
end
