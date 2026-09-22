# frozen_string_literal: true

# Bead ita-sgl, CONTROL: the softening is keyed on the three names the
# `Singleton` mixin really installs (`instance`, `_load`, `clone`).
# A misspelling of one of them is still a certain NoMethodError, so the
# class-object track must keep concluding on it.
require "singleton"

module Singleton
  def sgl_typo_duplicable?
    false
  end
end

class SglTypoTracker
  include Singleton
end

SglTypoTracker.instanse
