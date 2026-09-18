# The guard that keeps the new diagnostic from becoming a false-positive
# machine: a class object also answers to Class/Module/Object/Kernel, none of
# which any project index contains. Every call below is real and must stay
# silent — without the inventory check they would all be "undefined method".
class Page
  def initialize(slug)
    @slug = slug
  end
end

Page.name
Page.ancestors
Page.instance_methods
Page.superclass
Page.const_get(:X)
Page.respond_to?(:new)
Page.class_eval { nil }
