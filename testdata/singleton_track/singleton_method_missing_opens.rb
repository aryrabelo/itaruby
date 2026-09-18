# Shape (4): a singleton `method_missing` answers anything, so the class
# object's surface is unknowable and the class must be OPEN. Already
# handled (the `def` arm checks the NAME before it checks the track) -
# this is the proof.
class Ghost
  def self.method_missing(name, *args)
    return "ghost:#{name}" if name.to_s.start_with?("ghost_")
    super
  end

  def self.respond_to_missing?(name, include_private = false)
    name.to_s.start_with?("ghost_") || super
  end

  def self.known(a)
    a
  end
end

raise "expected method_missing to answer" unless Ghost.ghost_one == "ghost:ghost_one"
