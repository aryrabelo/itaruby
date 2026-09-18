# frozen_string_literal: true

ActiveRecord::Schema[7.1].define(version: 2024_01_01_000000) do
  enable_extension "pgcrypto"

  create_table "doohickeys", force: :cascade do |t|
    t.string "title"
    t.integer "quantity"
    t.decimal "price"
    t.boolean "published"
    t.datetime "launched_at"
    t.date "released_on"
    t.jsonb "metadata"
    t.index ["title"], name: "index_doohickeys_on_title"
  end

  create_table "sprockets", force: :cascade do |t|
    t.string "code"
    t.integer "count"
  end
end
