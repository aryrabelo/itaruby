# Bead ita-dpg.1: the project's own definition of a pack-declared
# namespace must win over the generated `rbs_collection.rbi` — `Redis` is
# a bare top-level entry there (`class Redis ; end`, permanently open,
# unknown method surface), but THIS project reopens it here with a real,
# closed ancestry (no `method_missing`, no reopening anywhere else in this
# fixture): an undefined method call must warn E0101, proving
# `merge_declared_fragment`'s "skip any path the project itself already
# defines" rule (`index.rs`) — the same design invariant as
# `declarations/gems.rbi`'s `ActiveRecord::Base`, checked from the
# opposite direction (closed project code, not an open declaration).
class Redis
  def real_method
    1
  end
end

class RbsPackRedefinitionUser
  def call
    Redis.new.totally_undefined_method
  end
end
