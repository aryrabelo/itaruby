# Wave 14 residual audit: every declaration added to
# crates/itaruby_semantic/declarations/gems.rbi in this round, one line per
# new namespace. Each was a measured E0104 on corpus-a or corpus-b that the
# scope audit proved scope cannot fix, and each name was read in its gem's
# own public source before curation (versions and files in gems.rbi's
# wave-14 comment). This whole file must be silent after: the constant
# resolves (kills E0104) and ancestry stays open, so the unknown method
# calls raise no false E0101 — same invariant as model.rb.
class Wave14NestedProbe
  def call
    Karafka.producer
    Karafka::Admin.read_lags_with_offsets({})
    Money::Currency.new(:usd)
    Flipper::Actor.new("user:1")
    Flipper::Adapters::ActiveRecord.new
    Flipper::Adapters::ActiveRecord::Gate.where(key: "x")
    ActiveStorage::FileNotFoundError.new("gone")
    ActiveResource::ConnectionError.new(nil)
    ActiveResource::UnauthorizedAccess.new(nil)
  end

  def rescued
    call
  rescue ActiveResource::UnauthorizedAccess
    nil
  rescue ActiveResource::ConnectionError => e
    e
  rescue ActiveStorage::FileNotFoundError => e
    e
  end
end
