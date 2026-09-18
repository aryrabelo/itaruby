# The control: the SAME class shape without the class-body loop. Nothing
# defines or forwards `foo`, the class is genuinely closed, so the bare
# self-send is a real latent NameError and must keep accusing — the fix
# for the loop shape must not blanket-silence its neighbours. Expected:
# exactly one E0101 at the `foo` call (12:5).
class NoLoopStillAccuses
  def initialize(lookup)
    @lookup = lookup
  end

  def render_thing
    foo
  end
end

NoLoopStillAccuses.new(nil).render_thing
