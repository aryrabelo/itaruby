# Control (bead ita-c8h): the PARENT namespace here (`GemSuperReopenApp`)
# IS defined by this project itself, via a bare `module` — the exact
# idiomatic Rails-generator shape (`module Admin; class FooController;
# ...; end; end`). A nested reopening under an ALREADY project-owned
# namespace must never be mistaken for an external gem's namespace: a
# genuine missing method here must keep accusing E0101 exactly as before
# this bead, or the mechanism would blanket-silence every namespaced
# controller/model tree a project writes this way.
module GemSuperReopenApp
end

class GemSuperReopenApp::Service
  def real_method
    1
  end
end

GemSuperReopenApp::Service.new.nonexistent_method
