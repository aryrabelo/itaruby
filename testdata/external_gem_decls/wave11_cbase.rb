# Wave 11 task item 3: does a leading `::` (cbase) resolve identically to
# the bare path once a curated declaration exists? `index.rs::resolve_const`
# strips a leading `::` and looks the rest up directly in `by_path`
# (`name.strip_prefix("::")` then `by_path.get(rest)`) — the exact same
# table `by_path.get(name)` reaches for the bare form's terminal fallback —
# so both spellings hit the identical `by_path["Stripe::Subscription"]`/
# `by_path["Stripe::Invoice"]` entry curated in gems.rbi. This file
# exercises both spellings of both names; if the answer were "no", exactly
# the `::`-prefixed lines below would still warn E0104 while the bare ones
# fell silent — that split is what the experiment is checking for.
class Wave11CbaseProbe
  def call
    Stripe::Subscription.retrieve("sub_1")
    ::Stripe::Subscription.retrieve("sub_1")
    Stripe::Invoice.retrieve("in_1")
    ::Stripe::Invoice.retrieve("in_1")
  end
end
