# Ignored by sorbet/config, real at runtime: the Rails boot loads every
# initializer. Defined ONLY here.
SearchClient = if ENV["SEARCH_URL"]
  Object.new
else
  nil
end
