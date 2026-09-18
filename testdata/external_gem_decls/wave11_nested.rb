# Wave 11 residual audit: every nested-member declaration added to
# crates/itaruby_semantic/declarations/gems.rbi in this round. Each was an
# E0104 in the five-corpus measurement (ruby-lsp, discourse, chatwoot,
# zammad, rails) before curation, absent from both gem_rbs_collection's own
# history AND the generated declarations/rbs_collection.rbi pack; this
# whole file must be silent after — the constant resolves (kills E0104)
# but ancestry stays open (no false E0101 on the unknown method call),
# same invariant as model.rb.
class Wave11NestedProbe
  def call
    Nokogiri::HTML5.parse("<a></a>")
    GraphQL::Types::ID.coerce_input("1", nil)
    GraphQL::CoercionError.new("bad")
    Faker::Number.between(from: 1, to: 10)
    Faker::Crypto.md5
    Faker::Time.forward
    Stripe::Subscription.retrieve("sub_1")
    Stripe::Invoice.retrieve("in_1")
    Stripe::Checkout::Session.retrieve("cs_1")
    Stripe::PromotionCode.retrieve("promo_1")
    Concurrent::CountDownLatch.new(1)
    Concurrent::CyclicBarrier.new(2)
    Concurrent::Event.new
    Concurrent::ThreadPoolExecutor.new
    RubyLLM::Message.new(role: :user, content: "hi")
    RubyLLM::Chat.new(model: "gpt")
    RubyLLM::Schema.create
    RubyLLM::Error.new("boom")
    ValidEmail2::Address.new("a@b.com")
    HTMLEntities.new.decode("&amp;")
    Elasticsearch::Client.new
    Elasticsearch::API::Indices::Actions.instance_method(:create)
    Twilio::REST::Client.new
    Twilio::REST::TwilioError.new("bad")
    Oj.dump({})
    Liquid::Template.parse("{{ x }}")
    RSpec::Matchers::DSL.instance_method(:matcher)
    RSpec::Expectations::ExpectationNotMetError.new("nope")
    RSpec::Core::Formatters::ConsoleCodes.wrap("x", :red)
  end
end
