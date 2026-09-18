# Bead ita-3gs mutation probe. `ActiveRecord::Base` and `Rails` are curated
# external declarations (crates/itaruby_semantic/declarations/gems.rbi):
# before this bead both were unresolved constants (E0104 x2); after, they
# resolve and this whole file is silent.
#
# `undefined_method` below has no local definition anywhere in this class
# and no known one on `ActiveRecord::Base` either — it must stay silent
# rather than become a false E0101, because the declaration never closes
# ancestry (invariant #1): we don't know if the real `ActiveRecord::Base`
# defines it via `method_missing` or metaprogramming.
class ExternalGemModel < ActiveRecord::Base
  def log_and_call
    Rails.logger.info("hello")
    undefined_method
  end
end
