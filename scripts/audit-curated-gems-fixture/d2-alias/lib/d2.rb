# Item 2: the `alias` keyword defines a method like `alias_method`.
class String
  alias d2_shout upcase
end

# Top level: a private `Object` method, like a top-level `def`.
alias d2_say puts
