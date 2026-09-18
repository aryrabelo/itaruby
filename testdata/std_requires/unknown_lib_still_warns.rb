# W3 control: requiring a lib that does not exist in the harvested stdlib
# inventory must not unlock anything — the require is inert for constant
# resolution, and the made-up constant keeps warning E0104 exactly as
# before this wave.
require 'totally_made_up_stdlib_xyz'

class StdReqUnknownLib
  def call
    TotallyMadeUpStdlibConst.fetch
  end
end
