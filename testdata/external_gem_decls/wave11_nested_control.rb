# A4 control fixture (wave 11 residual audit): `Stripe::Coupon` is a real,
# equally-public Stripe class under the SAME `Stripe` namespace this round
# curated four siblings under (`Subscription`/`Invoice`/`Checkout::Session`/
# `PromotionCode`) — and the same namespace the generated
# declarations/rbs_collection.rbi pack ALSO already covers nine other
# siblings of (`Charge`, `Customer`, `Event`, `PaymentIntent`,
# `PaymentMethod`, `Price`, `Product`, `Refund`, `Source`). `Coupon` itself
# is in neither: never measured (never appeared in the five-corpus
# residual), so never added to gems.rbi, and absent from the pack too. It
# must still warn E0104 — proof that neither gems.rbi nor the pack is a
# namespace-wide suppressor that opens every member once one sibling under
# `Stripe` is curated or generated.
class Wave11ControlProbe
  def call
    Stripe::Coupon.retrieve("cp_1")
  end
end
