# NEGATIVE CONTROL for the generalization this mechanism refuses.
# `elasticsearch-api` is in the lock and `api` is one of its hyphen
# segments, but `Api` here is the PROJECT's own namespace - so it must
# stay checked. Measured: segment matching would have blinded 145 such
# reopening sites in mastodon alone.
module Api
  class Client
    def call
      1
    end
  end
end

Api::Client.new.nonexistent_method
