# frozen_string_literal: true

# Bead ita-xta, FAIL-CLOSED side: when the extended module's own ancestry
# contains a module this index cannot enumerate — here a dynamic
# `define_method` whose name is computed — the class object's surface is
# UNREADABLE, not empty. `XtaOpaqueHost.xta_dynamic` must stay silent even
# though no literal definition of that name exists anywhere.
module XtaOpaqueSource
  %w[xta_dynamic].each do |n|
    define_method(n) { :dynamic }
  end
end

module XtaOpaqueMixin
  include XtaOpaqueSource
end

class XtaOpaqueHost
  extend XtaOpaqueMixin
end

raise "bad" unless XtaOpaqueHost.xta_dynamic == :dynamic
