# frozen_string_literal: true

# Bead ita-obx, the `Kernel` link of the same chain: `Object` includes
# `Kernel`, so a project `module Kernel` reopening is on every class
# object's dispatch too. Same monotonic direction — NotFound -> Found.
module Kernel
  def obx_kernel_probe
    :probe
  end
end

class Boulder; end

raise "bad" unless Boulder.obx_kernel_probe == :probe
