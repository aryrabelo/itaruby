# itaruby curated gem declarations (bead ita-3gs).
#
# Why this file exists: E0104 "unresolved constant" dominates all three
# measured corpora (corpus-a, corpus-b, corpus-c) via a small set of public
# gem namespaces this checker has never seen defined anywhere in the
# project — sorbet-runtime's `T` chief among them. Loading a project's own
# real RBI/RBS from disk is a separate, later capability (bead ita-vto);
# this file is itaruby's own curated allowlist, versioned in this repo and
# embedded into the binary via `include_str!` — no path discovered at
# runtime, no config, works on any machine.
#
# ENTRY RULE, applied by hand every time this file changes:
#   1. The name appeared in the frequency measurement of unresolved
#      constants taken across the three corpora (2026-08-20) — no
#      speculative entries. EXCEPTION (bead ita-h6l, 2026-08-24): a name
#      may also enter because it is one of the three name sources
#      `index.rs`'s `is_known_external_class_path` consults to detect a
#      project `class X ... end` as a REOPENING of an external class
#      rather than a new closed one (mechanism B). `Mail::Message`/
#      `Mail::Address` are the one measured case (rails/rails) that is
#      neither a Ruby core name nor a `declarations/stdlib_constants.txt`
#      entry — the `mail` gem is the only remaining source that can
#      resolve them. Every such entry still obeys rule 2 below.
#      EXCEPTION (bead ita-083, 2026-08-25): `T::Struct` and `TracePoint`
#      entered from the ita-40k head-to-head against Sorbet's own home
#      corpora (ruby-lsp, tapioca — both `typed: strict`), not the
#      original 2026-08-20 three-corpus scan: `T::Struct` is
#      sorbet-runtime's own struct base class (`class Foo < T::Struct`),
#      and `TracePoint` is used directly in typed:strict source
#      (tapioca/ruby-lsp both call it without ever `require`-ing or
#      defining it). Same rule 2 obligation — public API only.
#      EXCEPTION (2026-08-26, tapioca residual measurement): the
#      sorbet-runtime `T::*` namespace below, including `T::Private::*`.
#      `T::Private` is private TO THE GEM, not to one client's app —
#      tapioca (a public gem) legitimately patches sorbet-runtime
#      internals (`T::Private::Methods::DeclBuilder`,
#      `T::Private::Types::Void::Private::INSTANCE`), and rule 2's
#      anti-gaming clause is about app-private symbols, which these are
#      not. `T::Private::Types::Void::Private::INSTANCE` itself stays OUT:
#      it is a VALUE, and this file only carries class/module namespaces
#      (the `declares_open_namespaces_only_no_methods` contract test
#      forbids const-writes) — that one measured site is a documented
#      ceiling, not an oversight.
#      EXCEPTION (bead ita-dpg.3, 2026-08-26): the batch below is
#      test-framework-heavy (`Fabricate`, `RSpec`, `RSpec::Matchers`,
#      `FactoryBot`, `Test::Unit::TestCase`, `Minitest::Assertion`) —
#      that reflects what the five measured corpora (rails, discourse,
#      chatwoot, zammad, ruby-lsp) actually run in their own test
#      suites, not relaxed vetting. A test framework shipped as a gem is
#      exactly rule 2's target: every name below still ships in a
#      publicly released gem and none of them exists only in one
#      client's app, same bar as every non-test entry in this file.
#      EXCEPTION (wave 11 residual audit, 2026-08-26): 29 more entries,
#      each a NESTED member under a gem this checker had never seen any
#      part of, or under a top-level it already declared (`Stripe`,
#      `RubyLLM`, `RSpec`/`RSpec::Matchers` above) — measured across all
#      five public corpora (ruby-lsp, discourse, chatwoot, zammad,
#      rails), 1063 combined call sites, every one individually
#      grep-confirmed absent from a local scratch clone of
#      `gem_rbs_collection`'s ENTIRE git history (`git log --all
#      -S"<name>"`, not just its HEAD checkout) AND absent from the
#      generated `declarations/rbs_collection.rbi` pack — the
#      absence-proof itself was recorded in a local scratch artifact,
#      deliberately not versioned.
#      This pushes the file well past the roughly-20-entry ceiling the
#      earlier exceptions already bent (64 declared entries before this
#      batch): deliberate, not silent — the alternative
#      was leaving 1063 measured, real, non-app-internal sites warning
#      for a ceiling number with no enforcement mechanism of its own.
#      `GraphQL::Types::ID` is the real gem's own scalar under its own
#      namespace — NOT the bare `ID`/`Boolean`/`Int` local-alias pattern
#      rule 2 excludes below; that pattern is still excluded (measured
#      again this round: zammad's `app/graphql` `Boolean` alias, 215
#      sites, same shape, still not curated). The whole `Aws` namespace
#      (fragmented across dozens of exact paths, roughly 250 sites) is
#      excluded from this batch on purpose: `gem_rbs_collection` carried
#      full `aws-sdk` coverage (aws-sdk-core, aws-sdk-s3, aws-sdk-ec2 and
#      others) as recently as 2025 before dropping it (`git log
#      --diff-filter=D -- gems/aws-sdk-core` etc.) — "any version" per
#      rule 1's own absence bar means these are a collection regression
#      to report upstream, not a gap for this file to paper over by
#      hand.
#   2. The name is a public gem's own top-level API (sorbet-runtime, rails,
#      sidekiq, dry-initializer, i18n, money, flipper, appsignal,
#      sentry-ruby, turbo-rails, mail, fabrication, rspec-core,
#      rspec-expectations, factory_bot, multi_json, test-unit, minitest,
#      ruby-jwt, rotp, audited, logster, ruby_llm, omniauth, addressable,
#      loofah, koala, nokogiri, graphql, faker, stripe, concurrent-ruby,
#      valid_email2, htmlentities, elasticsearch, twilio, oj,
#      liquid) — never a symbol private to one client's app.
#      `ID`/`Boolean`/`Int` were measured too (corpus-c) but
#      excluded: they resolve to nothing anywhere inside the corpus's
#      checked `app/` tree, which is the signature of a local initializer
#      alias (a convention some graphql-ruby apps add for themselves), not
#      a constant the `graphql` gem itself provides at the top level —
#      declaring them would be exactly the app-symbol lookup table the
#      anti-gaming rule forbids, not a gem declaration.
#
# Every entry below is intentionally a bare, empty namespace: this resolves
# the CONSTANT reference (kills E0104) and nothing else. `ita check`'s
# ancestry resolver treats every one of these as permanently open (see
# `index.rs`'s `merge_declared_fragment`, which forces `open = true`
# unconditionally for every fragment parsed from this file) because we
# genuinely do not know whether the real gem class defines
# `method_missing`, metaprograms methods, or gets reopened with more
# behavior elsewhere — a project class inheriting from `ActiveRecord::Base`
# and calling an unknown method must stay silent (invariant #1), never
# become a false E0101. That openness is enforced entirely by the merge
# code in index.rs, not by anything written here (no `method_missing` shim
# needed in this file).

module T; end
module T::Array; end
module T::Hash; end
module T::Boolean; end
module T::Sig; end
class T::Struct; end
# sorbet-runtime's own namespaces beyond the core five above — measured
# 2026-08-26 on tapioca's residual E0104 (16 of 17 sites; the 17th is the
# INSTANCE value constant, deliberately undeclared — see the header).
# Intermediate namespaces (`T::Props`, `T::Private`, ...) exist because
# the resolution walk must resolve the OWNER before it can check the
# member. All remain empty and permanently open, same contract.
module T::Module; end
module T::Props; end
module T::Props::ClassMethods; end
module T::Private; end
module T::Private::Methods; end
class T::Private::Methods::DeclBuilder; end
module T::Private::Types; end
class T::Private::Types::Void; end
module T::Private::Types::Void::Private; end
module T::Private::Abstract; end
class T::Private::Abstract::Data; end
module T::Types; end
class T::Types::Proc; end
class T::Types::Base; end
module T::Sig::WithoutRuntime; end
module T::Set; end
module T::Enumerable; end

module Rails; end
class ActiveRecord::Base; end
class ActiveRecord::RecordNotFound; end
module ActiveSupport::Concern; end
class ActiveStorage::Blob; end
module Sidekiq::Worker; end
module Dry::Initializer; end
module I18n; end
class Money; end
module Flipper; end
module Appsignal; end
module Sentry; end
class Turbo::StreamsChannel; end
class Mail::Message; end
class Mail::Address; end
class TracePoint; end

# Batch measured 2026-08-26 across 5 public repos (rails, discourse,
# chatwoot, zammad, ruby-lsp) — E0104 sites for public gem namespaces no
# declaration file names today (bead ita-dpg.3): Fabricate 763, RSpec 685,
# RSpec::Matchers 54, FactoryBot 256, MultiJson 146, Test::Unit::TestCase
# 130, Minitest::Assertion 119, JWT 56, ROTP::TOTP 51, Audited::Audit 41,
# Logster 42, RubyLLM 39, OmniAuth::AuthHash 38, Addressable::URI 35,
# Loofah 35, Koala::Facebook::API 33. Bare parent namespaces (`Test`,
# `Test::Unit`, `Minitest`, `ROTP`, `Audited`, `OmniAuth`, `Addressable`,
# `Koala`, `Koala::Facebook`) are declared alongside their measured child
# even though the parent alone was not itself a counted site — same
# reason `T::Props`/`T::Private` above needed declaring: resolution
# walks the OWNER before it can check the MEMBER, so the member's exact
# path never resolves without it.
class Fabricate; end
module RSpec; end
module RSpec::Matchers; end
module FactoryBot; end
module MultiJson; end
module Test; end
module Test::Unit; end
class Test::Unit::TestCase; end
module Minitest; end
class Minitest::Assertion; end
module JWT; end
module ROTP; end
class ROTP::TOTP; end
module Audited; end
class Audited::Audit; end
module Logster; end
module RubyLLM; end
module OmniAuth; end
class OmniAuth::AuthHash; end
module Addressable; end
class Addressable::URI; end
module Loofah; end
module Koala; end
module Koala::Facebook; end
class Koala::Facebook::API; end

# Wave 11 residual audit (2026-08-26): nested members absent from BOTH
# gem_rbs_collection's entire history and the generated
# declarations/rbs_collection.rbi pack — see ENTRY RULE exception above
# and A-absence-proof.txt for the per-name proof of absence.
module Nokogiri::HTML5; end
class GraphQL::Types::ID; end
class GraphQL::CoercionError; end
class Faker::Number; end
class Faker::Crypto; end
class Faker::Time; end
class Stripe::Subscription; end
class Stripe::Invoice; end
class Stripe::Checkout::Session; end
class Stripe::PromotionCode; end
class Concurrent::CountDownLatch; end
class Concurrent::CyclicBarrier; end
class Concurrent::Event; end
class Concurrent::ThreadPoolExecutor; end
class RubyLLM::Message; end
class RubyLLM::Chat; end
class RubyLLM::Schema; end
class RubyLLM::Error; end
class ValidEmail2::Address; end
class HTMLEntities; end
class Elasticsearch::Client; end
module Elasticsearch::API::Indices::Actions; end
class Twilio::REST::Client; end
class Twilio::REST::TwilioError; end
module Oj; end
class Liquid::Template; end
module RSpec::Matchers::DSL; end
class RSpec::Expectations::ExpectationNotMetError; end
module RSpec::Core::Formatters::ConsoleCodes; end

# Wave 14 residual audit (2026-09-18): the DECLARATION family behind
# corpus-a/corpus-b's still-warning E0104 after the scope audit proved
# widening the mapped root fixes nothing here (11 of 30 audited sites were
# in-repo scope, 19 were declarations; measured: root scope silences 11 and
# adds 1195 unaudited sites, so subtrees stay). Each name below was READ in
# the gem's own public source before being written here — never from the
# corpus, which only says a name is unresolved, never that it exists:
#   karafka 2.6.1, lib/karafka/version.rb — `module Karafka` with
#     `class Admin` in lib/karafka/admin.rb (6 sites: 5 bare, 1 on Admin).
#   money 7.1.1, lib/money/version.rb — `class Money; class Currency` in
#     lib/money/currency.rb (2 sites). `Money` itself was already curated.
#   flipper 1.4.2, lib/flipper/version.rb — `class Actor` in
#     lib/flipper/actor.rb (1 site) and `class Gate < Model` in
#     lib/flipper/adapters/active_record/gate.rb (1 site), nested under
#     `module Adapters` / `class ActiveRecord` from
#     lib/flipper/adapters/active_record.rb. `Gate` carries the gem's own
#     "Private: Do not use outside of this adapter" comment: private TO THE
#     GEM, not to one client's app — the same distinction that let
#     `T::Private::*` in (see the ENTRY RULE's 2026-08-26 exception), and
#     rule 2's anti-gaming clause is about app-private symbols.
#   rails/rails main @ 8.2.0.alpha, activestorage/lib/active_storage/errors.rb
#     — `class FileNotFoundError < Error` (1 site, a rescue clause).
#   activeresource 6.2.0, lib/active_resource/version.rb — `class
#     ConnectionError < StandardError` and `class UnauthorizedAccess <
#     ClientError` in lib/active_resource/exceptions.rb (2 sites, both
#     rescue clauses: a rescue naming a real gem error must stay silent).
# Absent from `declarations/rbs_collection.rbi` (grepped per name): none of
# karafka, money, flipper or activeresource is in that pack's frozen gem
# list, and the pack's activestorage 7.0 entries carry no error classes.
# `Flipper::Adapters` / `Flipper::Adapters::ActiveRecord` and
# `ActiveResource` are intermediate owners, declared for the same reason
# `T::Props`/`Koala::Facebook` are: the resolution walk resolves the OWNER
# before it can check the member.
#
# DELIBERATELY OUT, one measured site each (documented ceilings, not
# oversights): `HTTParty::COMMON_NETWORK_ERRORS` is a VALUE constant (a
# frozen array), and this file carries only class/module namespaces — same
# ruling as `T::Private::Types::Void::Private::INSTANCE` above; the
# `HTTParty` module itself is already in the generated pack and declaring it
# does not resolve a member, because this is an exact-path allowlist with no
# prefix fallback. Five further sites are first-party PRIVATE-gem
# namespaces (an event-bus, a logging filter constant): they ship in no
# public gem, so they can never enter this file under rule 2 — they are a
# `sorbet/rbi`-style project-declaration capability (bead ita-vto), not a
# curation gap.
module Karafka; end
class Karafka::Admin; end
class Money::Currency; end
class Flipper::Actor; end
module Flipper::Adapters; end
class Flipper::Adapters::ActiveRecord; end
class Flipper::Adapters::ActiveRecord::Gate; end
class ActiveStorage::FileNotFoundError; end
module ActiveResource; end
class ActiveResource::ConnectionError; end
class ActiveResource::UnauthorizedAccess; end
