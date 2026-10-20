# frozen_string_literal: true

# 2026-10-19, RESOLUTION side (Homebrew/brew `extend/module.rb`): a
# project reopening of `Module` that INCLUDES a mixin puts the mixin's
# methods on every class object and every module object - `class Module;
# include T::Sig; end` is how brew makes `sig` callable in every class
# body. A PREPENDED mixin lands on the same dispatch. The class-object
# track must read the reopening's mixins: every call below resolves.
module MrmSigs
  def mrm_sig(*)
    :sig
  end
end

module MrmFirst
  def mrm_first
    :first
  end
end

class Module
  include MrmSigs
  prepend MrmFirst
end

class MrmPlank; end

module MrmBeam; end

raise "bad" unless MrmPlank.mrm_sig(:x) == :sig
raise "bad" unless MrmBeam.mrm_sig == :sig
raise "bad" unless MrmPlank.mrm_first == :first
