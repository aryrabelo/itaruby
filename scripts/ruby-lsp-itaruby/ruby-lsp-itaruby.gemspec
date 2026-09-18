# frozen_string_literal: true

require_relative "lib/ruby_lsp_itaruby/version"

Gem::Specification.new do |spec|
  spec.name = "ruby-lsp-itaruby"
  spec.version = RubyLspItaruby::VERSION
  spec.authors = ["itaruby"]
  spec.license = "MIT"
  spec.summary = "itaruby diagnostics addon for ruby-lsp"
  spec.description = "Plugs the itaruby Ruby type checker (`ita check`) into ruby-lsp as a " \
    "diagnostics formatter, using itaruby's whole-project inference instead of per-file heuristics."
  spec.homepage = "https://github.com/aryrabelo/itaruby"
  spec.required_ruby_version = ">= 3.0"

  spec.files = Dir["lib/**/*.rb", "README.md"]
  spec.require_paths = ["lib"]

  spec.add_dependency "ruby-lsp", ">= 0.23", "< 0.27"

  spec.metadata["source_code_uri"] = "https://github.com/aryrabelo/itaruby/tree/main/scripts/ruby-lsp-itaruby"
  spec.metadata["changelog_uri"] = "https://github.com/aryrabelo/itaruby/blob/main/docs/CHANGELOG.md"
  # Deleting this line is the deliberate act of publishing to rubygems.org.
  spec.metadata["allowed_push_host"] = "none — prototype, not published"
end
