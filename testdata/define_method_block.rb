class DefineMethodBlockSymbol
  define_method(:greet) do
    'hi'
  end
end

class DefineMethodBlockString
  define_method('greet') do
    'hi'
  end
end

class DefineMethodBlockDynamic
  name = :greet
  define_method(name) do
    'hi'
  end
end
