# W3 require/autoload: `JSON`/`JSON::ParserError` are stdlib constants
# (the harvested inventory records exactly which constants each stdlib
# require defines). The `require 'json'` in this very file makes them
# process-global per Ruby semantics, so both references resolve — this
# fixture must stay SILENT (no E0104). The require sits in the fixture
# itself so the isolated library test sees the same project the gate-c
# run sees.
require 'json'

class StdReqJsonUser
  def parse(text)
    JSON.parse(text)
  rescue JSON::ParserError => e
    e.message
  end
end
