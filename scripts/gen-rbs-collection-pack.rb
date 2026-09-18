#!/usr/bin/env ruby
# frozen_string_literal: true

# Generates crates/itaruby_semantic/declarations/rbs_collection.rbi (bead
# ita-dpg.1, extended by ita-dpg.2) from a local `gem_rbs_collection`
# checkout (https://github.com/ruby/gem_rbs_collection): text-parses every
# `gems/<name>/<latest-version>/**/*.rbs` for the frozen `GEMS` list below
# and emits a bare, empty `class`/`module` namespace per declared
# constant PATH — same suppression-only contract as the hand-curated
# `declarations/gems.rbi` (bead ita-3gs): resolves the constant (kills
# E0104) and nothing else. Bead ita-dpg.2 widens this to also harvest a
# value constant's NAME under a declared namespace (`Rack::CONTENT_TYPE`,
# `Nokogiri::XML::ParseOptions::NOBLANKS`) as the same kind of bare, empty
# entry — never its type. No methods, no sigs: mapping an `.rbs` method
# sig to itaruby's own method-signature model is a separate, later bead.
#
# Usage:
#   ruby scripts/gen-rbs-collection-pack.rb --self-test
#   ruby scripts/gen-rbs-collection-pack.rb <gem_rbs_collection_checkout> > crates/itaruby_semantic/declarations/rbs_collection.rbi
#
# Stdlib-only: no bundler, no gems beyond what ships with Ruby.

require "rubygems"

# Frozen gem list (measured, beads ita-dpg.1 + ita-dpg.2) — no
# speculative entries; every name here is checked against a real
# `gem_rbs_collection` checkout by `generate` (`gem directory not found`
# raises if one goes stale). `actionview` (21st of the original 20) is a
# deliberate addition beyond the first measurement: `actionpack`'s own
# `.rbs` files reference `ActionView::Layouts`/`ActionView::Base`/etc. via
# `include`/`extend` (real Rails coupling — `ActionController::Base`
# mixes in `ActionView::Layouts`) but never declare the `ActionView`
# namespace itself, so no amount of parsing `actionpack` alone can
# resolve those references — only `gem_rbs_collection`'s separate
# `actionview` gem directory declares it. `minitest` through `mail`
# (bead ita-dpg.2) are the residual-measurement top 120 gems the frozen
# 21-gem list simply never harvested; `mail` widens the two hand-curated
# `gems.rbi` entries (`Mail::Message`/`Mail::Address`) to the gem's real
# surface — those two hand entries still win on any path collision
# (`declared_fragments` parses `GEMS_RBI` first, and `merge_declared_fragment`/
# `entries.key?` here both keep first-seen). `cgi` has a
# `gem_rbs_collection` directory too but is stdlib, not a gem dependency —
# deliberately left OUT of this list; `core_stdlib_tops` below already
# excludes it by top segment on the rare chance a REOPENED-core `.rbs`
# ever nests something under it, so adding it here would buy nothing.
# Every entry earns its place by MEASUREMENT: a gem is listed only after a
# residual E0104 site on a public corpus was shown to resolve to a path this
# gem's collection directory actually declares, under the same rules this
# generator applies (latest version only, `_test/` fixtures excluded,
# core/stdlib tops filtered). Directory-exists is NOT the bar — see the
# `rake` case, a listed directory that yields zero entries because `Rake` is
# already in the stdlib inventory.
#
# Deliberately NOT listed: `cgi`. The collection has a `cgi` directory and it
# would address 115 measured sites, but `CGI` is stdlib and already in
# `declarations/stdlib_constants.txt`, where it is correctly gated on the
# project actually requiring it. Declaring it here would bypass that gate for
# a whole stdlib namespace.
GEMS = %w[
  faker rack graphql activerecord activesupport activemodel actionpack
  actionmailer activejob actioncable nokogiri redis i18n thor stripe
  httparty webmock pundit mysql2 concurrent-ruby actionview
  minitest jwt rake audited addressable faraday sidekiq sqlite3
  mini_mime web-push tzinfo rest-client commonmarker simplecov devise
  connection_pool mail
  rubyzip chunky_png line-bot-api marcel bcrypt globalid railties
  rails-html-sanitizer activestorage mini_magick googleauth
  google-cloud-errors rails-dom-testing listen rubocop mime-types fcm
  sidekiq-pro parallel sidekiq-ent delayed_job sidekiq-cron hashie
  geocoder browser stackprof slack-notifier rqrcode google-cloud-firestore
  google-cloud-ai_platform-v1 dogstatsd-ruby diffy delayed_job_active_record
].freeze

# A `class`/`module` ALIAS (`class NewName = ExistingName`) is a single
# RBS statement with no body and no matching `end` — measured once, live,
# in activesupport-generated.rbs (`class HashWithIndifferentAccess =
# ActiveSupport::HashWithIndifferentAccess`). Must be recognized and
# skipped BEFORE the open-block check below, or the real `end` that closes
# the enclosing module gets consumed by this phantom frame instead,
# corrupting every path nested after it in the same file.
ALIAS_RE = /\A(?:class|module)\s+[A-Za-z_][\w:]*\s*=/
# `interface` opens a block (and consumes an `end`) exactly like
# class/module, but is never itself a resolvable Ruby constant — pushed
# onto the nesting stack as a non-emitting frame so a real class/module
# declared *after* it (a sibling, once it closes) still nests correctly.
OPEN_RE = /\A(class|module|interface)\s+([A-Za-z_][\w:]*)/
END_RE = /\Aend\s*\z/
# A value constant declaration (`FOO: String`, `Owner::Sub::FOO: Integer`)
# — a plain `NAME: Type` line, RBS's syntax for a Ruby `NAME = ...`. Never
# confused with `class`/`module`/`interface` (those start with a lowercase
# keyword, checked first by `OPEN_RE`) or a method sig (`def foo: (...)
# -> ...`, also lowercase-first) or a `type` alias (lowercase `type`
# keyword). `(?!:)` after the colon rejects a `::` that would otherwise
# read as "one more path segment glued past the actual colon" on some
# malformed line — real value-constant colons are always followed by a
# type, never another `:`.
VALUE_CONST_RE = /\A([A-Z][A-Za-z0-9_]*(?:::[A-Z][A-Za-z0-9_]*)*)\s*:(?!:)/

# Every top-level constant name this checker ALREADY models itself, read
# from its own two generated inventories. A gem's `.rbs` freely REOPENS
# core/stdlib classes to declare the extensions it adds (measured live:
# activesupport's rbs reopens `String`, `Array`, `Hash`, `Object`,
# `Kernel`, `Time`, ...), and emitting those here would force
# `open = true` on them through `merge_declared_fragment` — silently
# destroying conclusive E0101 on every core receiver in every project
# (caught by `crates/itaruby/tests/agent_format.rs`, which stopped seeing
# `undefined method 'push' for 'String'`). Skipped by TOP SEGMENT, so a
# reopening's nested children go with it: the E0104 population this pack
# was measured against is gem namespaces, never core/stdlib, so the
# exclusion costs nothing it was built to buy.
DECLARATIONS_DIR = File.join(__dir__, "..", "crates", "itaruby_semantic", "declarations")
def core_stdlib_tops
  core = File.readlines(File.join(DECLARATIONS_DIR, "core_inventory.txt"), chomp: true)
             .reject { |l| l.start_with?("#") }
             .map { |l| l.delete_prefix("::").split("#").first.to_s.split("::").first }
  std = File.readlines(File.join(DECLARATIONS_DIR, "stdlib_constants.txt"), chomp: true)
             .reject { |l| l.start_with?("#") }
             .filter_map { |l| l.split("\t")[1]&.delete_prefix("::")&.split("::")&.first }
  # `is_known_core_constant`'s hand-written arm names constants no
  # inventory file carries (`BigDecimal`, `Net`, `OpenSSL`, ...). Scraped
  # from that one function's body so the Rust side stays the single
  # source of truth — duplicating the list here would let the two drift.
  rust = File.read(File.join(DECLARATIONS_DIR, "..", "src", "core.rs"))
             .split("pub fn is_known_core_constant")[1].to_s
             .split("\n}").first.to_s
             .scan(/"([A-Z][A-Za-z0-9_]*)"/).flatten
  (core + std + rust).compact.reject(&:empty?).to_set
end

# Parses one `.rbs` file's text, merging discovered `class`/`module`
# NAMESPACE paths AND value-constant NAMES into the shared `entries`
# (path -> is_module) and `order` (first-seen path order) — first
# declaration of a given path wins, mirroring `index.rs`'s own
# `intern`/`merge_declared_fragment` "first wins" rule. A value constant
# is stored with `entries[path] = false` (emits as `class ... ; end`,
# same bare-empty shape as a namespace — it is never actually a real
# Ruby class, but nothing downstream inspects that flag for a leaf
# constant path; only `resolve_const`'s `by_path` lookup matters).
#
# Deliberately still ignores everything else RBS can declare (`def`,
# `type` alias, `interface` member defs, `include`/`extend`/`prepend`,
# `attr_*`, `alias`) — none of those lines start with `class`/`module`/
# `interface`/`end`/an uppercase constant name immediately followed by a
# colon, so they never match any of the three regexes and fall through
# untouched. That is the whole suppression-only contract: only constant
# PATHS (namespace or value) are ever harvested, never a type, a method,
# or a sig.
def parse_rbs_text(text, entries, order)
  stack = []
  text.each_line do |raw|
    # RBS class/module/interface headers never carry `#` inside a string
    # literal, so truncating at the first `#` safely strips both whole
    # comment lines and trailing inline comments (measured live in
    # concurrent-ruby's `promises.rbs`) in one step — including a comment
    # that itself contains class/module-shaped text, e.g. `# class Fake`.
    stripped = raw.sub(/#.*/, "").strip
    next if stripped.empty?
    next if ALIAS_RE.match?(stripped)

    if (m = OPEN_RE.match(stripped))
      keyword, name = m[1], m[2]
      if keyword == "interface"
        stack.push(nil)
      else
        # A compact qualified name (`class Foo::Bar`) is already a full
        # top-level path in every occurrence measured across the 20
        # target gems (never nested inside another open block) — used
        # literally. A bare name nests under the innermost currently-open
        # real class/module frame (each stack entry already holds ITS OWN
        # full path, so only the last one is the correct prefix —
        # `interface` frames are `nil` and never become that prefix).
        parent = stack.compact.last
        full_path = name.include?("::") ? name : (parent ? "#{parent}::#{name}" : name)
        stack.push(full_path)
        unless entries.key?(full_path)
          entries[full_path] = (keyword == "module")
          order << full_path
        end
      end
      next
    end

    # Bead ita-dpg.2: `NAME: Type` — RBS's value-constant syntax. Checked
    # AFTER `OPEN_RE` (a `class`/`module` header never matches this: it
    # starts with a lowercase keyword) so there is no ordering conflict.
    # Same compact-qualified-vs-bare-name split as `OPEN_RE` above: a
    # qualified name (`Nokogiri::XML::Node::ENTITY_REF_NODE: Integer`,
    # written at a file's true top level with no enclosing block — the
    # measured real shape) is used exactly as written; a bare name
    # (`CONTENT_TYPE: String` inside `module Rack`) nests under the
    # innermost enclosing real class/module frame. A bare name with NO
    # enclosing frame (`stack.compact.last` nil — a genuinely top-level,
    # unqualified constant) has no owner and is dropped entirely: unlike
    # a class/module, a bare top-level value constant with nothing to
    # nest it is never a real gem API entry point worth resolving.
    if (m = VALUE_CONST_RE.match(stripped))
      name = m[1]
      owner = name.include?("::") ? nil : stack.compact.last
      full_path = name.include?("::") ? name : owner && "#{owner}::#{name}"
      if full_path && !entries.key?(full_path)
        entries[full_path] = false
        order << full_path
      end
      next
    end

    stack.pop if END_RE.match?(stripped)
  end
end

def latest_version_dir(gem_dir)
  versions = Dir.children(gem_dir).select do |entry|
    File.directory?(File.join(gem_dir, entry)) && entry =~ /\A\d+(\.\d+)*\z/
  end
  raise "no version directory found under #{gem_dir}" if versions.empty?

  versions.max_by { |v| Gem::Version.new(v) }
end

# Every `.rbs` file under `version_dir`, recursively, excluding
# `gem_rbs_collection`'s own `_test/` fixture directories (sig test
# corpora, not the gem's real published API) — sorted for a deterministic
# generation order.
def rbs_files(version_dir)
  Dir.glob(File.join(version_dir, "**", "*.rbs"))
     .reject { |f| f.split(File::SEPARATOR).include?("_test") }
     .sort
end

def generate(checkout_dir)
  entries = {}
  order = []
  resolved_versions = []
  GEMS.each do |gem|
    gem_dir = File.join(checkout_dir, "gems", gem)
    raise "gem directory not found: #{gem_dir}" unless File.directory?(gem_dir)

    version = latest_version_dir(gem_dir)
    resolved_versions << "#{gem} #{version}"
    rbs_files(File.join(gem_dir, version)).each do |file|
      parse_rbs_text(File.read(file), entries, order)
    end
  end

  puts <<~HEADER
    # itaruby curated rbs-collection constant declarations (bead ita-dpg.1).
    #
    # GENERATED FILE — do not hand-edit. Regenerate with:
    #   ruby scripts/gen-rbs-collection-pack.rb <gem_rbs_collection_checkout> \\
    #     > crates/itaruby_semantic/declarations/rbs_collection.rbi
    #
    # Source checkout: https://github.com/ruby/gem_rbs_collection (any local
    # clone works). The checkout's path is deliberately NOT recorded here:
    # it is machine-local, so embedding it both versions somebody's home
    # directory and makes this file differ per machine for no semantic
    # reason. What identifies the input is the resolved version per gem,
    # below. Only `gems/<name>/<version>/**/*.rbs` is read per gem, `_test/`
    # fixture directories excluded; the latest version directory present in
    # the checkout wins per gem:
    ##{" "}
    ##{"  "}#{resolved_versions.join(", ")}
    #
    # Frozen gem list (measured, beads ita-dpg.1 + ita-dpg.2) — no
    # speculative entries:
    #   #{GEMS.join(", ")}
    #
    # EXCLUDED: any path whose top-level segment this checker already
    # models itself (`declarations/core_inventory.txt`,
    # `declarations/stdlib_constants.txt`). Gem `.rbs` files reopen core
    # classes to declare their extensions (activesupport reopens `String`,
    # `Object`, `Kernel`, `Time`, ...), and declaring those here would
    # force `open = true` on them and destroy conclusive E0101 on every
    # core receiver — see `core_stdlib_tops`.
    #
    # SUPPRESSION-ONLY CONTRACT, identical to the hand-curated
    # `declarations/gems.rbi` (bead ita-3gs) this file sits beside: every
    # entry below is intentionally a bare, empty namespace. Parsed and
    # merged through the exact same path (`declarations.rs`'s
    # `parse_defs_text` + `index.rs`'s `merge_declared_fragment`), which
    # resolves the CONSTANT reference (kills E0104) and nothing else, force-
    # opens the namespace (we don't know the real gem's method surface —
    # invariant #1 forbids a false E0101), and skips any path the project
    # itself already defines (real project code always wins). No methods,
    # no sigs: mapping an `.rbs` method sig to itaruby's own
    # method-signature model is a separate, later bead (ita-3gs's
    # sig-mapping follow-up), not this one — declaring a method here
    # ahead of that bead would let this checker claim knowledge of gem
    # behavior it has not actually modeled, exactly the failure
    # `declares_open_namespaces_only_no_methods` polices in
    # `declarations.rs`. A value constant's NAME (`Rack::CONTENT_TYPE`,
    # `Nokogiri::XML::ParseOptions::NOBLANKS`) IS harvested (bead
    # ita-dpg.2) — same bare, empty, suppression-only entry as a
    # namespace, never its type.
  HEADER
  puts

  excluded = core_stdlib_tops
  order.each do |path|
    next if excluded.include?(path.split("::").first)

    keyword = entries[path] ? "module" : "class"
    # Space before `;` (unlike `gems.rbi`'s `module T; end`) so a bare
    # top-level entry with no `::`-nested children (e.g. `WebMock`) still
    # has a word-boundary character right after its name — otherwise a
    # coverage check grepping for `^(class|module) Name(::|$| )` can only
    # ever match through a coincidentally-present nested entry.
    puts "#{keyword} #{path} ; end"
  end
end

# ---------------------------------------------------------------------
# --self-test: inline fixtures covering every parsing rule above,
# entirely independent of any local gem_rbs_collection checkout.
# ---------------------------------------------------------------------

SELF_TEST_CASES = [
  {
    name: "nested_modules",
    rbs: <<~RBS,
      module A
        module B
          class C
          end
        end
      end
    RBS
    expected: [%w[A module], ["A::B", "module"], ["A::B::C", "class"]],
  },
  {
    name: "compact_qualified_class",
    rbs: <<~RBS,
      class Foo::Bar
        def baz: () -> void
      end
    RBS
    expected: [["Foo::Bar", "class"]],
  },
  {
    name: "superclass_dropped_generics_dropped",
    rbs: <<~RBS,
      class Widget[T] < SomeGem::Base[T]  # trailing comment, generics on both sides
      end
    RBS
    expected: [%w[Widget class]],
  },
  {
    name: "type_alias_skipped",
    rbs: <<~RBS,
      module Gem1
        type foo = Integer | String
        class Real
        end
      end
    RBS
    expected: [%w[Gem1 module], ["Gem1::Real", "class"]],
  },
  {
    name: "interface_skipped_nesting_stays_correct",
    rbs: <<~RBS,
      module Gem2
        interface _Foo
          def call: () -> void
        end
        class Real
        end
      end
    RBS
    expected: [%w[Gem2 module], ["Gem2::Real", "class"]],
  },
  {
    # Bead ita-dpg.2: a bare value constant (`FOO: String`) directly
    # inside a `module`/`class` block now HARVESTS the name, nested under
    # that enclosing owner — flips the old "value constants skipped"
    # behavior for exactly this shape (real measured family:
    # `Rack::CONTENT_TYPE`, `Rack::SET_COOKIE`).
    name: "value_constant_with_owner_harvested",
    rbs: <<~RBS,
      module Gem3
        VERSION: String
        class Real
        end
      end
    RBS
    expected: [%w[Gem3 module], ["Gem3::VERSION", "class"], ["Gem3::Real", "class"]],
  },
  {
    # Bead ita-dpg.2: the OTHER real measured shape — a fully qualified
    # value constant written at true top level, no enclosing block at
    # all (`Nokogiri::XML::Node::ENTITY_REF_NODE: Integer`, `Nokogiri::
    # XML::ParseOptions::NOBLANKS: Integer`, both literal in nokogiri's
    # own `.rbs`). Used exactly as written, mirroring `OPEN_RE`'s own
    # compact-qualified-class rule.
    name: "compact_qualified_value_constant_harvested",
    rbs: <<~RBS,
      Gem6::Sub::CONST_NAME: Integer
      module Gem6
        class Real
        end
      end
    RBS
    expected: [["Gem6::Sub::CONST_NAME", "class"], %w[Gem6 module], ["Gem6::Real", "class"]],
  },
  {
    # Bead ita-dpg.2: a BARE (unqualified) value constant with no
    # enclosing block at all has no owner to nest under and must be
    # dropped — never emitted bare, unlike a class/module.
    name: "top_level_bare_value_constant_excluded",
    rbs: <<~RBS,
      TOP_LEVEL_CONST: String
      module Gem7
        class Real
        end
      end
    RBS
    expected: [%w[Gem7 module], ["Gem7::Real", "class"]],
  },
  {
    # Bead ita-dpg.2: value-constant harvesting must not widen to swallow
    # a `type` alias or a method sig sitting right next to it in the same
    # owner — both keep falling through untouched.
    name: "value_constant_does_not_swallow_method_or_type_alias",
    rbs: <<~RBS,
      module Gem8
        FOO: String
        type Bar = Integer | String
        def baz: () -> void
        class Real
        end
      end
    RBS
    expected: [%w[Gem8 module], ["Gem8::FOO", "class"], ["Gem8::Real", "class"]],
  },
  {
    name: "comment_lines_with_class_like_text_ignored",
    rbs: <<~RBS,
      # class FakeClass should never be parsed
      # module FakeModule end
      module Gem4
        # class StillFake
        class Real
        end
      end
    RBS
    expected: [%w[Gem4 module], ["Gem4::Real", "class"]],
  },
  {
    name: "class_alias_skipped_does_not_eat_the_real_end",
    rbs: <<~RBS,
      module Gem5
        class Real
        end
        class RealAlias = Gem5::Real
        class AfterAlias
        end
      end
    RBS
    expected: [%w[Gem5 module], ["Gem5::Real", "class"], ["Gem5::AfterAlias", "class"]],
  },
].freeze

def run_self_test
  failures = []
  SELF_TEST_CASES.each do |c|
    entries = {}
    order = []
    parse_rbs_text(c.fetch(:rbs), entries, order)
    actual = order.map { |p| [p, entries[p] ? "module" : "class"] }
    expected = c.fetch(:expected).map { |p, k| [p, k] }
    failures << "#{c[:name]}: expected #{expected.inspect}, got #{actual.inspect}" if actual != expected
  end

  total = SELF_TEST_CASES.size
  passed = total - failures.size
  failures.each { |f| warn "FAIL: #{f}" }
  puts "self-test: #{passed}/#{total} passed, #{failures.size} fail"
  exit(failures.empty? ? 0 : 1)
end

if $PROGRAM_NAME == __FILE__
  if ARGV.first == "--self-test"
    run_self_test
  elsif ARGV.first && !ARGV.first.empty?
    generate(ARGV.first)
  else
    warn "usage: #{$PROGRAM_NAME} --self-test | <gem_rbs_collection_checkout>"
    exit 1
  end
end
