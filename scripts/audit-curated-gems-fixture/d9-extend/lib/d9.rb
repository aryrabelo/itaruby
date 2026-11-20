module D9; end
String.extend(D9)
class Integer
  extend D9
end
Object.const_get(:Hash).extend(D9)
