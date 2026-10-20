# frozen_string_literal: true

# 2026-10-19, CONTROL: the core-reopening mixin consult is keyed on the
# names the mixin literally defines. `mrm_sgi` is a misspelling of the one
# name `MrmSpells` installs on every class object, a certain
# NoMethodError, so the class-object track must keep concluding on it.
module MrmSpells
  def mrm_sig(*)
    :sig
  end
end

class Module
  include MrmSpells
end

class MrmPost; end

MrmPost.mrm_sgi
